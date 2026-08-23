# jungle-wasm

`jungle-wasm` is the browser boundary for the authoritative Rust Jungle engine.
It contains no rule implementation: game state and legality come from
`jungle-core`, while all move selection comes from `jungle-search`.

Build the generated web module with:

```powershell
wasm-pack build engine/crates/jungle-wasm --target web --release
```

The default files are `jungle_wasm.js` and `jungle_wasm_bg.wasm`.

## JavaScript ABI

The generated module exports `WasmGameSession`. Its string-based JSON boundary
keeps the Worker protocol independent from wasm-bindgen object marshalling:

- `new WasmGameSession()` creates one authoritative game and persistent search
  owner.
- `load_tablebase(bytes, descriptorJson)` eagerly validates and installs the
  manifest-declared exact two-piece oracle or an additive sparse shard. Search
  remains unavailable until the mandatory legacy capability reports success.
- `load_tablebase_shard(bytes, descriptorJson)` is the explicit optional-shard
  alias. Sparse installs are atomic/idempotent and capped at 32 MiB resident
  bytes per search lane; the authority session should remain mandatory-only.
- `initial_snapshot_json()` returns the current snapshot.
- `handle(commandJson)` accepts the legacy browser commands plus
  `commitAiMove`.
- `search(commandJson, progressCallback?)` searches a detached `gameState`
  without mutating the session. The callback receives a JSON string after each
  fully completed depth.
- `cancel_search(searchId)` cooperatively marks a search cancelled. Terminating
  a disposable search Worker is the hard-cancel fallback without shared Wasm
  memory.
- `cancellation_flag_ptr()` exposes the atomic cancellation word for builds
  whose thread glue shares Wasm memory.
- `configure_search_lane(laneId)` gives an independent search Worker a stable,
  deterministic Lazy-SMP ordering lane. The coordinator only advertises
  Lazy-SMP after more than one lane is ready.
- `configure_runtime_features(json)`, `capabilities_json()`, and
  `engine_metadata_json()` expose scalar/SIMD/thread feature negotiation.

Detached searches and `commitAiMove` carry `searchId`, `baseRevision`, and a
16-character lowercase hexadecimal `positionHash`. The authority rejects stale
or mismatched commits and revalidates the chosen move before changing state.
