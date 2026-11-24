#[path = "../engine/crates/jungle-cli/src/stats.rs"]
pub mod stats;
use baseline_search::{NoopControl, SearchConfig, SearchEngine, SearchLimits};
use jungle_core::Side;
use jungle_entry::{Ply, Snapshot};
use serde_json::json;
use stats::{CandidateOutcome as Outcome, PairScore, paired_bootstrap};
use std::{
    fs,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn game(start: &Snapshot, candidate: Side, seed: u32, budget: u32) -> (Outcome, u32, String) {
    let mut s = start.clone();
    while s.status == "playing" {
        let moves = s.legal().unwrap();
        let pick = if s.to_play == candidate {
            jungle_entry::fixed(&s, &moves, "hard", seed, budget)
                .unwrap()
                .index
        } else {
            let mut engine = SearchEngine::new(SearchConfig::hard());
            engine.configure_lane(seed % 4);
            let choice = engine
                .choose_move(
                    &s.position().unwrap(),
                    SearchLimits::fixed_nodes(budget.into()),
                    &NoopControl,
                )
                .unwrap();
            assert!(choice.nodes <= u64::from(budget));
            let m = Ply::from(choice.best_move.unwrap());
            moves.iter().position(|p| *p == m).unwrap()
        };
        s = s.apply(moves[pick]).unwrap();
    }
    (
        match s.winner {
            None => Outcome::Draw,
            Some(w) if w == candidate => Outcome::Win,
            _ => Outcome::Loss,
        },
        s.rev,
        s.reason,
    )
}
fn main() {
    let pairs = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(200usize);
    let started = Instant::now();
    let work = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let threads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .clamp(1, 4);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(||loop{
        let i=work.fetch_add(1,Ordering::Relaxed);if i>=pairs{break}
        let mut seed=0x096abea5417_u64.wrapping_mul(1+i as u64);
        let mut opening=Snapshot::opening(Side::Red);
        for _ in 0..(6+i%20){let legal=opening.legal().unwrap();if legal.is_empty(){break}let m=legal[random(&mut seed) as usize%legal.len()];let next=opening.apply(m).unwrap();if next.status!="playing"{break}opening=next;}
        let budget=if i%2==0{1000}else{5000};
        let red=game(&opening,Side::Red,seed as u32,budget);let blue=game(&opening,Side::Blue,seed as u32,budget);
        let score=PairScore{candidate_as_blue:blue.0,candidate_as_red:red.0};
        let mut rows=results.lock().unwrap();rows.push((i,budget,score,json!({"pair":i,"budget":budget,"opening":opening.history,"seed":seed as u32,"red":red,"blue":blue})));
        if rows.len()%20==0{eprintln!("{} / {pairs} pairs",rows.len());}
    });
        }
    });
    let mut rows = results.into_inner().unwrap();
    rows.sort_by_key(|r| r.0);
    let summaries=[0,1000,5000].map(|budget|{let ss:Vec<_>=rows.iter().filter(|r|budget==0||r.1==budget).map(|r|r.2).collect();
        let mut red=stats::WinDrawLoss::default();let mut blue=stats::WinDrawLoss::default();
        for p in &ss{red.record(p.candidate_as_red);blue.record(p.candidate_as_blue);}
        json!({"nodes":budget,"pairs":ss.len(),"red":red,"blue":blue,"score":ss.iter().copied().map(PairScore::score).sum::<f64>()/ss.len() as f64,"interval":paired_bootstrap(&ss,10000,20261120)})});
    let report = json!({"baseline":"72ca198","elapsedSeconds":started.elapsed().as_secs_f64(),"threads":threads,"summary":summaries,"games":rows.into_iter().map(|r|r.3).collect::<Vec<_>>()});
    fs::create_dir_all("docs/evidence").unwrap();
    fs::write(
        "docs/evidence/strength.json",
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", report["summary"]);
}
