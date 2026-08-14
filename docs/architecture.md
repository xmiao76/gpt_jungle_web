# Architecture

## Trust boundary

The browser has one game authority: `BrowserGameSession` in `python/browser_bridge.py`, running under Pyodide in `src/worker/game.worker.ts`. React never derives legal moves, capture eligibility, ranks, jumps, turns, winners, or terminal reasons. Its terrain constants are presentation metadata only.

The retained source packages are `jungle/domain`, `jungle/rules`, `jungle/engine`, and `jungle/ai`, including `ai/data/two_piece_v1.jgtb`. They match desktop commit `b0bb427458a686aa0ed10770bda50f9311a7845b` byte-for-byte according to `python/source-manifest.json`. Desktop Tkinter/controller/packaging modules are excluded.

`python/web_ai` is a clearly separated browser-only Hard-search overlay, currently identified as `web-hard-v2`. Easy and Medium continue to dispatch to the locked desktop implementation. Hard reuses the same rules, `GameState`, search configuration, Zobrist scheme, transposition-table implementation, and exact tablebase while replacing only its compact position/evaluation/search hot path. Random reachable-position parity tests compare legal move order, make/unmake state, and hashes against the locked compact core. The desktop repository and manifest-locked payload remain untouched.

## Runtime sequence

1. React creates a module Worker and sends `init` with the current static asset base URL.
2. The Worker loads pinned Pyodide 314.0.3 from jsDelivr.
3. It fetches the same-origin runtime manifest and content-hashed Python zip.
4. SHA-256 and byte size are checked with Web Crypto before extraction to `/runtime`.
5. `/runtime` is prepended to `sys.path`; the Worker imports `browser_bridge` and creates one session.
6. Session construction loads and verifies the exact tablebase through the unchanged `importlib.resources` path and creates one session-scoped `web-hard-v2` search owner.
7. Only then does the Worker publish a ready snapshot.

The loading UI reports runtime, engine, session/tablebase, and ready stages. A failed initialization destroys and recreates the Worker when Retry is selected.

## Protocol and concurrency

`src/protocol.ts` defines discriminated request/response envelopes plus runtime guards. Every message carries a monotonically increasing request ID and session revision. The client tracks pending calls, rejects malformed envelopes, rejects state replies from stale revisions, and distinguishes fatal Worker initialization errors from recoverable command errors.

The Python search is intentionally synchronous inside the Worker; no main-thread frame is blocked. React disables state-mutating input while search is active. AI-vs-AI submits exactly one move at a time, updates the screen, yields to the browser, and decides whether to submit another. Stop therefore takes effect between searches. Undo/redo uses the Python `Game` stacks; paired human/AI undo records how many plies were removed so redo restores that exact line.

## State ownership

| Concern | Owner |
| --- | --- |
| Board, side to move, history, legal moves, result | Python session |
| AI profile, search, diagnostics, tablebase | Python session; web-only overlay for Hard |
| Undo/redo stacks and save-state validation | Python session |
| Selection and legal-target rendering | React, from Worker response |
| Dialogs, audio, responsive layout, animation | React |
| Board orientation | React presentation state |
| AI-vs-AI scheduling | React, one authoritative request at a time |

Canonical board indices are invariant. In normal orientation display index `v` is canonical `v`; flipped orientation maps to `62 - v`. Input, focus, pieces, labels, highlights, and animations use the same mapping. Flipping never alters ownership, turn, history, or Python state.

## Build and hosting

The deterministic archive builder first verifies the retained desktop sources, then packages them with `browser_bridge.py` and `python/web_ai`. It sorts paths, fixes zip timestamps/permissions, omits transient files, and emits `public/generated/python-runtime.<hash>.zip` plus a small runtime manifest. Two builds from identical sources produce identical bytes and names. Vite uses relative asset URLs and emits a static `dist`; `public/_headers` makes hashed assets immutable while keeping HTML and the runtime manifest revalidatable.

Cloudflare Pages serves that directory with no server-side code. Pyodide runtime files come from the pinned official jsDelivr package; the game engine, bridge, art, archive, and tablebase are same-origin static assets.

## Save validation

The UI first checks JSON syntax and the versioned wrapper. Python then constructs a candidate `GameState`, validates board length and enums, duplicate animal identities, water/den occupancy, alternating move history, turn parity, replay consistency, and terminal evaluation. The live session is replaced only after all checks succeed. Imports clear undo/redo stacks by constructing a new `Game`, as the portable format stores game state but not command stacks.
