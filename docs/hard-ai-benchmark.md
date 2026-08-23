# Hard AI validation

## What counts as strength

Engine v2 does not promote on nodes per second, nominal depth, or a fixed list
of preferred moves. Those remain useful diagnostics, but they are not comparable
across Python, native Rust, scalar Wasm, SIMD Wasm, classical evaluation, and a
quantized evaluator.

The frozen initial champion is `web-hard-v2` at the same 2,000 ms budget as the
candidate. The Python champion is exposed through
`python -m tools.legacy_engine_jsonl`; production does not ship it.

## Fast gates

Every pull request runs:

- authoritative perft and rule matrices;
- make/unmake, incremental hash, and terminal invariants;
- exact tablebase samples and artifact fingerprint checks;
- deterministic fixed-node search tests;
- a small paired arena smoke corpus;
- scalar/Wasm protocol, deadline, and browser responsiveness tests.

The smoke arena catches gross regressions but does not make an Elo claim.
`npm run benchmark:ai` uses one color-swapped pair at 80 ms per move so it stays
fast enough for local iteration; the shipping promotion match remains 2,000 ms.

## Champion match

A promotion run must supply up to 500 nonterminal, nonduplicate holdout openings
stratified across early and middle-game plies. Tuning, book construction,
evaluator training, and parameter selection must not read that corpus.

Each opening produces a pair of games with candidate and champion colors
reversed. Both engines receive warmed artifacts and the same two-second clock on
a pinned browser and dedicated host. A crash, illegal move, corrupt result, or
missed hard deadline is a game loss and a release failure. A game still
nonterminal after 256 plies scores one half only inside the arena.

Matches run in 50-pair batches. Evaluation begins after 200 pairs and ends by
500 pairs. The paired opening—not an individual game—is the bootstrap sampling
unit. Promotion requires:

1. the lower endpoint of the paired two-sided 95% bootstrap interval above 50%
   (stricter than a one-sided 95% gate);
2. raw score of at least 45% from each color;
3. zero illegal moves, crashes, protocol failures, or deadline losses.

An inconclusive result at 500 pairs does not promote. The SIMD/threaded and
scalar fallback tiers maintain independent champion records.

## Artifacts

Every arena result is emitted as structured JSON with candidate/champion engine
identities, versioned corpus identity, limits, seed, W/D/L by color, paired
confidence interval, failure counts, per-game termination, and aggregate search
telemetry. A full promotion record should add immutable binary/corpus hashes and
host/browser metadata in its retained CI artifact.

## Legacy comparison

The original 20-position benchmark and initial-position throughput numbers are
kept as historical data only. Several cases previously returned without search
or hit the old depth-10 ceiling long before the clock expired, and the old depth
result varied with host load. Engine v2 removes that ceiling and searches until
its soft deadline, a proven result, an exact tablebase result, cancellation, or
the hard stop.
