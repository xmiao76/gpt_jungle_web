# Release validation

## Production

- Target URL: <https://gpt-jungle.pages.dev>
- Cloudflare Pages project: `gpt-jungle`
- Production branch: `main`
- Deployment mode: authenticated Wrangler Direct Upload (static assets only)
- Deployment state: live and verified
- Public verification date: 2026-08-13 (America/Los_Angeles)
- Deployed source commit: `cf38f824a0da50a56a2d25666f50d91752a3518a`
- Verified preview deployment: <https://0aa6cdc0.gpt-jungle.pages.dev>
- Verified immutable production deployment: <https://6afd6ed7.gpt-jungle.pages.dev>
- GitHub CI: [passed in 4m58s](https://github.com/xmiao76/gpt_jungle_web/actions/runs/31774741717)

The requested Git integration could not be attached to the exact project through the available credential: Wrangler's OAuth session supported Pages CLI operations, but the source-enabled public API returned 401 and no standalone API-token environment credential existed. Creating the project with Wrangler therefore selected Direct Upload, which Cloudflare does not allow converting to Git integration later. The repository retains a credential-free validation workflow, and releases use the documented manual command.

## Automated release gate

- Local validation date: 2026-08-13
- Status: passed
- CPython: 106 passed
- Vitest / React Testing Library: 29 passed across 10 files
- Playwright: 17 passed, 16 intentional non-Chromium skips across Chromium, Firefox, and WebKit
- Public production Playwright: 3 passed against the hook-free Pages deployment
- Deterministic Python archive: `python-runtime.3d6bfb6025f189ec.zip`, 97,617 bytes
- Hard AI benchmark: 20/20 tactical/endgame cases, 1.339× median legacy throughput, depth 6 versus depth 5 at equal 1,800 ms budgets

`npm run test:all` runs, in order:

1. copied-source manifest and SHA-256 verification;
2. CPython tests for domain, rules, engine, AI, tablebase, full games, and browser adapter;
3. ESLint with zero warnings;
4. Vitest/React Testing Library tests;
5. a normal production build;
6. a test-enabled production build and real Pyodide Worker flows in Chromium, Firefox, and WebKit.

The browser suite checks runtime readiness, the initial board, legal selection and a human/AI turn, canonical flip invariance, AI-first behavior, paired undo/redo and JSON persistence, authoritative trap coordinates, selected-piece stacking, main-thread responsiveness during Hard search, a known tablebase hit, an authentic deterministic terminal AI game, and a terminal move/result dialog fixture. The standalone `npm run benchmark:ai` gate compares the isolated `web-hard-v2` overlay with the manifest-locked legacy Hard search.

## Browser performance record

Benchmark host used during implementation:

- CPU: AMD Ryzen 5 PRO 2400G with Radeon Vega Graphics
- OS: Windows 10 Pro 10.0.19045
- Browser: Playwright Chromium 151.0.7922.34
- Position: standard initial setup, AI controlling Blue, Hard profile
- Search budget: 1,800 ms; acceptance envelope: at most 2,200 ms reported search time

Observed on 2026-08-13 from the standard initial position:

| Depth | Nodes | Nodes/s | TT hits | TB hits | Search time |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 5 | 21,312 | 11,830 | 9,060 | 0 | 1,801.5 ms |

The final full-gate run sampled 52.0 animation frames/s during search and opened the interactive How-to-Play dialog in 82 ms. A separate deterministic two-piece fixture asserts nonzero exact tablebase hits; the initial 16-piece position is not tablebase-eligible. Search node counts vary with machine load because the engine correctly searches until the fixed wall-clock budget rather than enforcing a fixed depth. The reproducible CPython comparison and its interpretation are recorded in [Hard AI benchmark](hard-ai-benchmark.md).

## Public post-deploy checklist

- [x] Root HTML, JavaScript, CSS, PNG art, runtime manifest, and Python archive return successfully.
- [x] Pyodide modules and WASM load without CSP, CORS, MIME, or 404 failures.
- [x] Worker reaches ready with Python 3.14.2, web engine 0.2.0, Hard AI `web-hard-v2`, and tablebase loaded.
- [x] No uncaught console exceptions occur.
- [x] Human Starts completes a human move and legal AI reply.
- [x] AI Starts assigns the human Red and completes Blue's move without blocking UI.
- [x] A complete deterministic two-piece game reaches an authentic engine terminal result.
- [x] Save/load and paired undo/redo preserve an exact position.
- [x] Flip preserves canonical state; diagnostics and AI statistics populate.
- [x] AI-vs-AI completes a game; local regression coverage verifies stop-after-current-search; mute toggles safely.
- [x] Desktop and 390 px layouts keep the full board and controls usable without horizontal scrolling.

The public harness also asserts that production contains no `window.__JUNGLE_TEST__` hook. Its network audit observed hashed JavaScript/CSS/PNG assets, the runtime JSON, content-hashed Python zip, Pyodide WASM, and no response at status 400 or higher.

Both the preview and production hostname passed all three public Playwright cases. A direct post-deploy manifest check confirmed that the production alias and immutable deployment serve `python-runtime.3d6bfb6025f189ec.zip` with SHA-256 `3d6bfb6025f189ecd4bac2ab40f3107fb98c7ea031d4c04ac437ce7cbbe189a9`.
