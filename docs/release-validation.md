# Release validation

## Production

- Target URL: <https://gpt-jungle.pages.dev>
- Cloudflare Pages project: `gpt-jungle`
- Production branch: `main`
- Deployment state: pending the authenticated release step for this implementation
- Public verification date: pending

This document is updated after the public deployment is exercised. A local pass alone is never recorded as public verification.

## Automated release gate

- Local validation date: 2026-08-13
- Status: passed
- CPython: 98 passed
- Vitest / React Testing Library: 25 passed across 9 files
- Playwright: 16 passed, 14 intentional non-Chromium skips across Chromium, Firefox, and WebKit
- Deterministic Python archive: `python-runtime.943d395228327cba.zip`, 90,371 bytes

`npm run test:all` runs, in order:

1. copied-source manifest and SHA-256 verification;
2. CPython tests for domain, rules, engine, AI, tablebase, full games, and browser adapter;
3. ESLint with zero warnings;
4. Vitest/React Testing Library tests;
5. a normal production build;
6. a test-enabled production build and real Pyodide Worker flows in Chromium, Firefox, and WebKit.

The browser suite checks runtime readiness, the initial board, legal selection and a human/AI turn, canonical flip invariance, AI-first behavior, paired undo/redo and JSON persistence, main-thread responsiveness during Hard search, a known tablebase hit, an authentic deterministic terminal AI game, and a terminal move/result dialog fixture.

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
| 4 | 15,520 | 8,621 | 6,148 | 0 | 1,800.2 ms |

The final full-gate run sampled 57.1 animation frames/s during search and opened the interactive How-to-Play dialog in 88 ms; an isolated repeat sampled 60.4 frames/s. A separate deterministic two-piece fixture asserts nonzero exact tablebase hits; the initial 16-piece position is not tablebase-eligible. Search node counts vary with machine load because the desktop engine correctly searches until the fixed wall-clock budget rather than enforcing a fixed depth.

## Public post-deploy checklist

- [ ] Root HTML, JavaScript, CSS, PNG art, runtime manifest, and Python archive return successfully.
- [ ] Pyodide modules and WASM load without CSP, CORS, MIME, or 404 failures.
- [ ] Worker reaches ready with Python 3.14, engine 0.1.0, and tablebase loaded.
- [ ] No uncaught console exceptions occur.
- [ ] Human Starts completes a human move and legal AI reply.
- [ ] AI Starts assigns the human Red and completes Blue's move without blocking UI.
- [ ] A complete deterministic game reaches `den_entry`, `capture_all`, or `no_legal_moves` from the engine.
- [ ] Save/load and paired undo/redo preserve an exact position.
- [ ] Flip preserves canonical state; diagnostics and AI statistics populate.
- [ ] AI-vs-AI starts and stops after the active search; audio/mute toggles safely.
- [ ] Desktop and narrow responsive layouts keep the full board and controls usable without horizontal scrolling.
