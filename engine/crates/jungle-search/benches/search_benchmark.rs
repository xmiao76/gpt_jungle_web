fn main() {
    use std::time::Instant;

    use jungle_core::Position;
    use jungle_search::{NoopControl, SearchConfig, SearchEngine, SearchLimits};

    const NODES: u64 = 250_000;
    let position = Position::initial();
    let mut engine = SearchEngine::new(SearchConfig::hard());
    let started = Instant::now();
    let result = engine
        .choose_move(&position, SearchLimits::fixed_nodes(NODES), &NoopControl)
        .expect("benchmark position is valid");
    let elapsed = started.elapsed().as_secs_f64();
    assert!(result.best_move.is_some());
    assert!(result.nodes <= NODES);
    println!(
        "jungle-search nodes={} depth={} wall_ms={:.3} wall_nps={:.0} tt_hits={}",
        result.nodes,
        result.depth,
        elapsed * 1_000.0,
        result.nodes as f64 / elapsed.max(f64::EPSILON),
        result.transposition_hits,
    );
}
