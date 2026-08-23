use jungle_core::Move;
use serde::{Deserialize, Serialize};

pub const MAX_SEARCH_DEPTH: u8 = 128;
pub const DEFAULT_SOFT_TIME_MS: u32 = 1_950;
pub const DEFAULT_HARD_TIME_MS: u32 = 2_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchTermination {
    MaxDepth,
    SoftDeadline,
    HardDeadline,
    NodeLimit,
    Cancelled,
    Proven,
    Terminal,
    NoLegalMove,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchLimits {
    pub soft_time_limit_ms: u32,
    pub time_limit_ms: u32,
    pub node_limit: Option<u64>,
    pub max_depth: u8,
}

impl SearchLimits {
    pub const fn exact_time(time_limit_ms: u32) -> Self {
        let reserve = if time_limit_ms >= 1_000 {
            50
        } else {
            time_limit_ms / 40
        };
        Self {
            soft_time_limit_ms: time_limit_ms.saturating_sub(reserve),
            time_limit_ms,
            node_limit: None,
            max_depth: MAX_SEARCH_DEPTH,
        }
    }

    pub const fn fixed_nodes(node_limit: u64) -> Self {
        Self {
            soft_time_limit_ms: u32::MAX,
            time_limit_ms: u32::MAX,
            node_limit: Some(node_limit),
            max_depth: MAX_SEARCH_DEPTH,
        }
    }

    pub fn normalized(self) -> Self {
        let time_limit_ms = self.time_limit_ms.max(1);
        Self {
            soft_time_limit_ms: self.soft_time_limit_ms.min(time_limit_ms),
            time_limit_ms,
            node_limit: self.node_limit,
            max_depth: self.max_depth.clamp(1, MAX_SEARCH_DEPTH),
        }
    }
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            soft_time_limit_ms: DEFAULT_SOFT_TIME_MS,
            time_limit_ms: DEFAULT_HARD_TIME_MS,
            node_limit: None,
            max_depth: MAX_SEARCH_DEPTH,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchDiagnostics {
    pub qnodes: u64,
    pub evaluations: u64,
    pub tt_probes: u64,
    pub tt_hits: u64,
    pub tt_cutoffs: u64,
    pub tt_stores: u64,
    pub tt_collisions: u64,
    pub hashfull_per_mille: u16,
    pub tablebase_hits: u64,
    pub cycle_hits: u64,
    pub beta_cutoffs: u64,
    pub aspiration_researches: u64,
    pub pvs_researches: u64,
    pub lmr_researches: u64,
    pub futility_prunes: u64,
    pub razor_prunes: u64,
    pub delta_prunes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgress {
    pub depth: u8,
    pub selective_depth: u8,
    pub nodes: u64,
    pub elapsed_ms: f64,
    pub score: i32,
    pub best_move: Option<Move>,
    pub principal_variation: Vec<Move>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub depth: u8,
    pub selective_depth: u8,
    pub nodes: u64,
    pub elapsed_ms: f64,
    pub nodes_per_second: u64,
    pub transposition_hits: u64,
    pub tablebase_hits: u64,
    pub principal_variation: Vec<Move>,
    /// Every fully completed iterative-deepening result. Parallel browser lanes
    /// merge only records from a depth completed by every participating lane.
    #[serde(default)]
    pub iterations: Vec<SearchProgress>,
    pub termination: SearchTermination,
    pub diagnostics: SearchDiagnostics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_exactly_the_shipping_budget() {
        let limits = SearchLimits::default();
        assert_eq!(limits.soft_time_limit_ms, 1_950);
        assert_eq!(limits.time_limit_ms, 2_000);
        assert_eq!(limits.max_depth, 128);
    }

    #[test]
    fn constructors_distinguish_time_and_node_modes() {
        let timed = SearchLimits::exact_time(2_000);
        assert_eq!(
            (timed.soft_time_limit_ms, timed.time_limit_ms),
            (1_950, 2_000)
        );
        assert_eq!(timed.node_limit, None);

        let nodes = SearchLimits::fixed_nodes(12_345);
        assert_eq!(nodes.node_limit, Some(12_345));
        assert_eq!(nodes.time_limit_ms, u32::MAX);
    }

    #[test]
    fn normalization_clamps_invalid_public_values() {
        let limits = SearchLimits {
            soft_time_limit_ms: 50,
            time_limit_ms: 0,
            node_limit: None,
            max_depth: 0,
        }
        .normalized();
        assert_eq!(limits.time_limit_ms, 1);
        assert_eq!(limits.soft_time_limit_ms, 1);
        assert_eq!(limits.max_depth, 1);
    }

    #[test]
    fn progress_move_uses_the_browser_object_shape() {
        let movement = jungle_core::Position::initial().legal_moves().as_slice()[0];
        let progress = SearchProgress {
            depth: 1,
            selective_depth: 1,
            nodes: 2,
            elapsed_ms: 0.25,
            score: 7,
            best_move: Some(movement),
            principal_variation: vec![movement],
        };
        let json = serde_json::to_value(&progress).expect("serialize progress");
        assert!(json.get("selectiveDepth").is_some());
        let encoded = &json["principalVariation"][0];
        for field in [
            "origin",
            "destination",
            "piece",
            "captured",
            "is_jump",
            "note",
        ] {
            assert!(encoded.get(field).is_some(), "missing Move field {field}");
        }
        let decoded: SearchProgress = serde_json::from_value(json).expect("deserialize progress");
        assert_eq!(decoded, progress);
    }
}
