# Release validation

## Engine-v2 status

The Rust/Wasm implementation was validated locally on 2026-08-23
(America/Los_Angeles) and deployed to the existing production project.

- Project: `gpt-jungle`
- Production branch: `main`
- Deployment mode: Wrangler Direct Upload
- Deployment ID: `a82ba5c7-e9c0-4d58-b0f4-0d489757dc1d`
- Immutable URL: <https://a82ba5c7.gpt-jungle.pages.dev>
- Production alias: <https://gpt-jungle.pages.dev>
- Cloudflare source metadata: `b35fb46` with the working tree marked dirty
- Public verification date: 2026-08-23 (America/Los_Angeles)

Deployment remains static Cloudflare Pages with no Functions, Workers, storage,
bindings, secrets, remote engine, or telemetry. `npm run deploy` has a
`predeploy` production build so a test-hook build cannot be uploaded by
accident.

## Complete local gate

`npm run test:all` passed and left a normal hook-free production build in
`dist`. The gate covers:

- the 18-file frozen Python source manifest and 110 Python reference tests;
- Rust formatting and Clippy with warnings denied;
- 98 Rust tests across the canonical rules/session, search, tablebase, Wasm
  bridge, JSONL server, and subprocess arena;
- six deterministic artifact/build tests;
- ESLint, TypeScript compilation, and 42 Vitest/React tests across 13 files;
- real built-Wasm Playwright flows: 17 passed and 16 intentional
  non-Chromium skips across Chromium, Firefox, and WebKit;
- a final scalar/threaded Wasm build, integrity manifest, and production Vite
  bundle.

The browser suite exercises runtime selection, mandatory tablebase validation,
canonical rules, human/AI turns, search identity, four-lane Lazy SMP where the
host supports it, exact deadlines, progress, immediate hard cancellation,
undo/redo, save-v1 round trips, historyless puzzles, board orientation,
responsive geometry, terminal UI, and a complete exact-tablebase game.

## Browser performance record

Implementation host:

- CPU: AMD Ryzen 5 PRO 2400G with Radeon Vega Graphics
- OS: Windows 10 Pro 10.0.19045
- Browser: Playwright Chromium 151.0.7922.34
- Position: standard initial setup, AI controlling Blue
- Hard budget: 1,950 ms soft / 2,000 ms hard
- Runtime tier: SIMD/atomics artifact, four independent Lazy-SMP lanes

Observed final-gate result:

| Depth | Aggregate nodes | Nodes/s | TT hits | TB hits | Search time |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 9 | 2,227,168 | 1,113,473 | 383,384 | 0 | 2,000.2 ms |

The main thread sustained 54.4 animation frames/s during search, and the
How-to-Play interaction completed in 76 ms. The initial 16-piece position is
not tablebase-eligible; a separate two-piece browser fixture verifies nonzero
exact hits. Node totals vary with host load because each lane searches until
the wall-clock stop and only completed iterations are eligible for selection.

## Strength diagnostic

`npm run benchmark:ai` runs a quick equal-time, color-swapped subprocess pair
against the frozen `web-hard-v2` reference. The final 80 ms smoke run produced:

- candidate score: 100%;
- two wins and zero draws or losses;
- 100% from each color;
- zero illegal moves, deadline losses, crashes, or protocol failures.

This one-pair smoke result is not an Elo estimate or promotion. A release that
claims a new champion still requires an untuned holdout corpus, at least 200
pairs, the paired confidence/color gates, and zero strict failures documented
in [Hard AI validation](hard-ai-benchmark.md).

## Artifact record

- Runtime manifest schema: `gpt-jungle-engine-runtime` v2
- Engine ABI: `jungle-wasm-v2`
- Rules fingerprint:
  `bd5e29e5baf50114aa5ce62f7bfaee325a4aabc4ba69e7b983f3ba79c00a0c3a`
- Runtime manifest SHA-256:
  `292b3746573a4d0827b81bfaff2b2871f801599a1c569e65520bc5071aebf302`
- Published engine/tablebase bytes: 1,929,193
- Mandatory two-piece tablebase SHA-256:
  `a32b620d22d1735ef4f2a13b8d1388c07ad45151060f638c2434db988f97ece5`
- Frozen Python reference commit:
  `b0bb427458a686aa0ed10770bda50f9311a7845b`

Generated runtime assets are ignored build outputs. The production bundle
contains the Rust/Wasm variants, exact tablebase, and runtime manifest; the old
Pyodide archive is removed during every engine build.

## Post-deploy verification

The three hook-free public Playwright scenarios passed against both the
immutable URL and production alias. The deployed manifest is 2,747 bytes and
matches the local SHA-256 recorded above at both endpoints.

- [x] COOP/COEP/CORP headers produce the expected capability tier.
- [x] Manifest, selected Wasm module, and mandatory tablebase return without
      integrity, CORS, MIME, or status errors.
- [x] No production `window.__JUNGLE_TEST__` hook exists.
- [x] Human-first and AI-first games complete legal replies.
- [x] Save/load, paired undo/redo, orientation, diagnostics, and mute work.
- [x] A deterministic two-piece game reaches an authentic terminal result.
- [x] Desktop and 390 px layouts remain usable without horizontal scrolling.
