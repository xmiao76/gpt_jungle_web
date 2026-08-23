# Rust/Wasm engine v2

Engine v2 replaces the production Pyodide search loop with one Rust rules and
search implementation compiled both natively and to WebAssembly. The native
build powers deterministic tests, self-play, tablebase generation, and champion
matches. The Wasm build is the only game authority shipped to browsers.

## Compatibility contract

- Canonical board indices remain 0 through 62, with Red at the top and Blue at
  the bottom. Board flipping is still presentation-only.
- `gpt-jungle-save` version 1 remains the import/export format. Imported state
  is reconstructed and validated; search caches and artifacts are never saved.
- The `easy`, `medium`, and `hard` identifiers remain stable. Their default
  budgets are 250 ms, 750 ms, and 2,000 ms respectively.
- Engine v2 preserves this project's custom rules: Dog rank 3, Wolf rank 4,
  Lion jumps both river spans, Tiger jumps only the two-square span, and play
  has no repetition or no-capture adjudication.

Moves after a terminal outcome and coordinates outside 0 through 62 are now
rejected at every checked boundary. These are correctness fixes rather than
rules changes.

## Runtime boundary

The React application retains presentation state. An authority Worker owns the
Rust session, while disposable search Workers operate on immutable position
snapshots. Search responses carry the search ID, base revision, and position
hash; the authority revalidates all three and the returned move before applying
it. Cancelling search never destroys undo, redo, or save state.

Cloudflare Pages remains a static host. All executable and strength artifacts
are same-origin and content-hashed. Cross-origin isolation enables the
SIMD/atomics artifact and a bounded pool of independent Lazy-SMP search Workers;
feature detection selects the scalar single-lane fallback when unavailable.

## Strength artifacts

The artifact schema reserves model and book kinds, while this release consumes
the mandatory two-piece tablebase plus optional exact tablebase shards. Every
asset is bound to the engine ABI and rules fingerprint. The browser cache is a
250 MiB IndexedDB LRU; optional fetching is asynchronous and never extends the
two-second move clock.

Tablebases contain exact WDL plus distance for complete material signatures.
Graph-theoretic draws mean neither player can force a terminal win; they remain
an internal search value and do not add a draw result to browser gameplay.

## Promotion policy

`web-hard-v2` at 2,000 ms is the first frozen champion. Candidate and champion
play paired games from the same holdout openings with colors reversed. After at
least 200 pairs, promotion requires all of the following:

- the lower endpoint of a paired two-sided 95% interval is above 50%;
- each color scores at least 45%;
- no illegal move, crash, artifact error, or deadline loss occurs.

Inconclusive matches continue to 500 pairs and do not promote if still
inconclusive. A 256-ply draw is an arena-only measurement rule; product gameplay
continues to follow the no-draw ruleset. Threaded/SIMD and scalar fallback builds
have separate champion ladders.

## Local commands

The root npm scripts orchestrate the pinned toolchains. Rust-specific commands
can also be run directly:

```powershell
cargo test --manifest-path engine/Cargo.toml --workspace
wasm-pack build engine/crates/jungle-wasm --target web --release
python -m tools.legacy_engine_jsonl
```

Generated Wasm, manifests, books, models, and tablebase shards are build outputs
under `public/generated` and are not committed.
