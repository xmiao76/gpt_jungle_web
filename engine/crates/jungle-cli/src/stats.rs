use serde::{Deserialize, Serialize};

pub const DEFAULT_BOOTSTRAP_SAMPLES: u32 = 100_000;
pub const DEFAULT_BOOTSTRAP_SEED: u64 = 0x4a55_4e47_4c45_5632;
pub const MAX_BOOTSTRAP_SAMPLES: u32 = 1_000_000;
pub const MINIMUM_PROMOTION_PAIRS: u32 = 200;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateOutcome {
    Win,
    Draw,
    Loss,
}

impl CandidateOutcome {
    #[must_use]
    pub const fn points(self) -> f64 {
        match self {
            Self::Win => 1.0,
            Self::Draw => 0.5,
            Self::Loss => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WinDrawLoss {
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

impl WinDrawLoss {
    pub fn record(&mut self, outcome: CandidateOutcome) {
        match outcome {
            CandidateOutcome::Win => self.wins += 1,
            CandidateOutcome::Draw => self.draws += 1,
            CandidateOutcome::Loss => self.losses += 1,
        }
    }

    #[must_use]
    pub const fn games(self) -> u32 {
        self.wins + self.draws + self.losses
    }

    #[must_use]
    pub fn score(self) -> f64 {
        let games = self.games();
        if games == 0 {
            return 0.0;
        }
        (f64::from(self.wins) + f64::from(self.draws) * 0.5) / f64::from(games)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairScore {
    pub candidate_as_blue: CandidateOutcome,
    pub candidate_as_red: CandidateOutcome,
}

impl PairScore {
    #[must_use]
    pub fn score(self) -> f64 {
        (self.candidate_as_blue.points() + self.candidate_as_red.points()) * 0.5
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInterval {
    pub confidence: f64,
    pub lower: f64,
    pub upper: f64,
    pub samples: u32,
    pub seed: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionAssessment {
    pub ready: bool,
    pub required_lower_bound: f64,
    pub minimum_color_score: f64,
    pub minimum_pairs: u32,
    pub reasons: Vec<String>,
}

#[derive(Clone, Copy, Debug)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, length: usize) -> usize {
        debug_assert!(length > 0);
        (self.next() % length as u64) as usize
    }
}

/// Returns a deterministic percentile bootstrap over paired opening scores.
///
/// Sampling whole pairs preserves the correlation between the two color-swapped
/// games from an opening. The interval must never be computed over individual
/// games, because doing so overstates the effective sample size.
#[must_use]
pub fn paired_bootstrap(pairs: &[PairScore], samples: u32, seed: u64) -> Option<BootstrapInterval> {
    if pairs.is_empty() || samples == 0 {
        return None;
    }

    let pair_scores: Vec<f64> = pairs.iter().copied().map(PairScore::score).collect();
    let mut rng = SplitMix64::new(seed);
    let mut distribution = Vec::with_capacity(samples as usize);
    for _ in 0..samples {
        let total: f64 = (0..pair_scores.len())
            .map(|_| pair_scores[rng.index(pair_scores.len())])
            .sum();
        distribution.push(total / pair_scores.len() as f64);
    }
    distribution.sort_unstable_by(f64::total_cmp);

    let last = distribution.len() - 1;
    let lower_index = ((last as f64) * 0.025).floor() as usize;
    let upper_index = ((last as f64) * 0.975).ceil() as usize;
    Some(BootstrapInterval {
        confidence: 0.95,
        lower: distribution[lower_index],
        upper: distribution[upper_index.min(last)],
        samples,
        seed,
    })
}

#[must_use]
pub fn assess_promotion(
    interval: Option<BootstrapInterval>,
    candidate_as_blue: WinDrawLoss,
    candidate_as_red: WinDrawLoss,
    failures: u32,
) -> PromotionAssessment {
    const REQUIRED_LOWER_BOUND: f64 = 0.5;
    const MINIMUM_COLOR_SCORE: f64 = 0.45;

    let mut reasons = Vec::new();
    let completed_pairs = candidate_as_blue.games().min(candidate_as_red.games());
    if completed_pairs < MINIMUM_PROMOTION_PAIRS {
        reasons.push(format!(
            "completed {completed_pairs} opening pairs; promotion requires at least {MINIMUM_PROMOTION_PAIRS}"
        ));
    }
    match interval {
        Some(value) if value.lower > REQUIRED_LOWER_BOUND => {}
        Some(value) => reasons.push(format!(
            "paired two-sided 95% interval lower endpoint {:.6} does not exceed {:.2}",
            value.lower, REQUIRED_LOWER_BOUND
        )),
        None => reasons.push("no completed opening pairs".to_owned()),
    }
    if candidate_as_blue.games() == 0 || candidate_as_blue.score() < MINIMUM_COLOR_SCORE {
        reasons.push(format!(
            "candidate Blue score {:.6} is below {:.2}",
            candidate_as_blue.score(),
            MINIMUM_COLOR_SCORE
        ));
    }
    if candidate_as_red.games() == 0 || candidate_as_red.score() < MINIMUM_COLOR_SCORE {
        reasons.push(format!(
            "candidate Red score {:.6} is below {:.2}",
            candidate_as_red.score(),
            MINIMUM_COLOR_SCORE
        ));
    }
    if failures > 0 {
        reasons.push(format!("candidate recorded {failures} strict failures"));
    }

    PromotionAssessment {
        ready: reasons.is_empty(),
        required_lower_bound: REQUIRED_LOWER_BOUND,
        minimum_color_score: MINIMUM_COLOR_SCORE,
        minimum_pairs: MINIMUM_PROMOTION_PAIRS,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wdl_score_counts_draws_as_half_a_point() {
        let score = WinDrawLoss {
            wins: 2,
            draws: 1,
            losses: 1,
        };
        assert_eq!(score.games(), 4);
        assert_eq!(score.score(), 0.625);
    }

    #[test]
    fn paired_bootstrap_is_deterministic_and_preserves_degenerate_scores() {
        let wins = vec![
            PairScore {
                candidate_as_blue: CandidateOutcome::Win,
                candidate_as_red: CandidateOutcome::Win,
            };
            12
        ];
        let first = paired_bootstrap(&wins, 1_000, 7).expect("non-empty pairs");
        let second = paired_bootstrap(&wins, 1_000, 7).expect("non-empty pairs");
        assert_eq!(first, second);
        assert_eq!((first.lower, first.upper), (1.0, 1.0));
    }

    #[test]
    fn promotion_requires_confidence_color_balance_and_no_failures() {
        let interval = BootstrapInterval {
            confidence: 0.95,
            lower: 0.55,
            upper: 0.70,
            samples: 100_000,
            seed: 1,
        };
        let healthy = WinDrawLoss {
            wins: 140,
            draws: 40,
            losses: 20,
        };
        assert!(assess_promotion(Some(interval), healthy, healthy, 0).ready);
        let assessment = assess_promotion(Some(interval), healthy, healthy, 1);
        assert!(!assessment.ready);
        assert!(
            assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("strict failures"))
        );
    }
}
