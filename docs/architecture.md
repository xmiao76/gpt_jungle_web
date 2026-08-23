# Architecture

## Trust and state ownership

The browser has one game authority: a `WasmGameSession` in the authority module
Worker. React never derives turns, legal moves, captures, effective ranks,
jumps, winners, or terminal reasons. Its terrain data is presentation metadata.

The authority and native tooling share the `jungle-core` crate. It owns the
compact position, custom rules, legal move generation, checked move application,
outcomes, incremental hash, move log, undo/redo, and version-1 DTO validation.
There is no second optimized rule implementation for search to drift from.

| Concern | Owner |
| --- | --- |
| Position, rules, legal moves, result | Rust `jungle-core` |
| Undo/redo, import/export, revision | authority Worker session |
| Search, evaluation, TT, diagnostics | disposable search Worker(s) |
| Tablebase cache and artifact manifest | engine manifest + browser artifact cache |
| Selection, dialogs, animation, audio | React |
| Orientation | React presentation state |
| AI-vs-AI scheduling | React, one authoritative commit at a time |

Canonical board indices are always Red-at-top/Blue-at-bottom. Flipped display
index `v` maps to canonical index `62 - v`; flipping cannot change game state.

## Search isolation

An AI request snapshots the compact position and records a search ID, session
revision, and Zobrist key. Search runs away from the authority event loop so it
can be cancelled without destroying the session. A result may be committed only
when all three identifiers still match and the returned move is legal in the
authoritative position.

Hard search has a 1.95-second soft deadline and a 2.00-second atomic stop. It
returns the last fully completed iterative-deepening result. Search-path cycles
have a neutral internal value, carry a history-dependent marker, and do not
produce reusable transposition bounds or a gameplay draw result.

The fast artifact is selected only when cross-origin isolation, shared memory,
and SIMD are available. The coordinator runs independent deterministic
Lazy-SMP Web Workers (no shared-TT claim), capped at four and always leaving one
reported hardware thread for the UI. Runtime detection falls back to one scalar
lane without changing rules, saves, or the public protocol.

## Runtime sequence

1. React creates the authority Worker and sends `init` with the static asset URL.
2. The Worker fetches the no-cache engine runtime manifest.
3. It verifies the manifest schema, engine ABI, rules fingerprint, artifact size,
   and SHA-256 metadata.
4. Capability checks select the threaded/SIMD or scalar Wasm module.
5. The coordinator initializes the supported search lanes; every lane and the
   authority validate the mandatory exact two-piece tablebase.
6. The Worker publishes a ready snapshot with the actual lane count and
   language-neutral capabilities.
7. Optional complete tablebase groups are prefetched asynchronously into the
   250 MiB IndexedDB LRU. Only the current material group, capped at 32 MiB per
   auxiliary lane, may become resident; lane zero never waits for it.

Initialization failure destroys the Worker. Retry creates a new Worker and
authority instance.

## Protocol

The existing request commands remain stable: `init`, `newGame`, `legalTargets`,
`applyMove`, `aiMove`, `undo`, `redo`, `importGame`, and `exportGame`. Envelopes
retain monotonically increasing IDs and revisions. Additive search metadata
includes the search ID, base revision, position hash, progress, stop reason,
principal variation, node counters, TT statistics, tablebase hits, participating
lanes, and runtime capability tier.

The UI-facing `GameSnapshot`, canonical move objects, difficulties, and version-1
save wrapper remain language-neutral. Search or runtime objects never cross the
boundary.

## Artifacts and build

The Rust workspace produces native tools plus scalar and threaded web-target
Wasm packages. A deterministic builder copies these and exact tablebase assets
under `public/generated`, rejects any individual artifact at or above the Pages
file limit, and emits a content-hashed manifest with:

- engine ABI and rules fingerprint;
- artifact URL, byte size, SHA-256, and kind;
- complete material-signature metadata for tablebase shards;
- measured probe value used to select shards under the cache budget.

Cloudflare Pages remains a static Direct Upload project. `_headers` provides
COOP, COEP, CORP, no-cache HTML/manifest responses, and immutable generated
assets. There are no Functions, remote engine endpoints, bindings, or secrets.

## Save validation

Version-1 saves are parsed into checked DTOs. The engine validates the board,
piece uniqueness, terrain occupancy, turn/history consistency, legal replay,
and the first terminal result before constructing a session. Redundant stored
winner/result text cannot override the canonical derived outcome. Valid
historyless positions remain supported for puzzles; moves made after such an
import retain their live UI log but serialize historyless because save v1 has no
field for a nonstandard history origin. Imported sessions begin with empty
undo/redo stacks as before.

## Reference boundary

The manifest-locked Python engine remains development-only as the frozen
`web-hard-v2` champion and differential referee. `tools/legacy_engine_jsonl.py`
exposes it through the same language-neutral arena protocol. Python and Pyodide
are not packaged into production engine artifacts.
