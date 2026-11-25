# Jungle 2.0.0 release validation

The rebuilt component reports model **GPT 6.2 Astra**, title **GPT 6.2 Astra · Codex**, platform `official` and version `2.0.0`. The hosted game and single-file HTML embed those same metadata and component bytes.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `release/gpt-6-2-astra-codex-2.0.0.wasm` | 452,804 | `cc17fc4561a2ee32790fd66006ead24a833192e2e0947c850ddf6e1735019b34` |
| `release/index.html` | 1,241,292 | `d485e9679b2f48f5f13d68813850e41b1133700e6082fa9168da60282d2e14f8` |

`npm run verify:release` verified both checksums and the embedded component. `npm run test:release` instantiated the renamed component, verified its identity and passed admission, rule, render and strategy checks. [Rebuilt component measurements](evidence/admission.json)

## Original 2.0.0 build

The remaining measurements describe the original **GPT-6 · Codex** build. Its artifacts and all original browser and admission records are available together at commit `84a8bfa`. Validation used Windows 10, Rust 1.94.1, wasm-tools 1.263.0, wasm-bindgen-cli 0.2.130, Node 26.13.1 and jco-transpile 0.24.0. [Original validation record](https://github.com/xmiao76/gpt_jungle_web/blob/84a8bfa/docs/release-validation.md)

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `release/gpt-6-codex-2.0.0.wasm` | 452,804 | `4cafc7f31b48503660da34fc5c1e62a50605c91249b70067baca091de7378034` |
| `release/index.html` | 1,241,268 | `375a68e2a1a761d5cfcb82d07870ec4df0a504b39376404e9a560bda168c3146` |

## Rules, strategy and component

The release build checked the SHA-256 of the canonical rules profile against `a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57`. The independent referee and Rust engine agreed on both openings, all 23 portable legality cases, capture and quiet counters, outcome precedence and repetition.

The component's expanded WIT matched the checked-in world. Transpilation took 579 ms. The fresh-instance admission smoke test took 133 ms. On the opening, Hard used 45 ms at 20,000 nodes and 1.51 s at 500,000 nodes. Geometry, opacity, flip, deterministic choice, isolated instances, capture animation and direct replay were checked against the release file. [Original admission measurements](https://github.com/xmiao76/gpt_jungle_web/blob/84a8bfa/docs/evidence/admission.json)

The independent browser referee ran three Medium matches at 20,000 nodes, seed 20261120. Both seats drew the opening and every subsequent revision, including the result.

| GPT-6 · Codex seat | Opponent | Result | Plies | Frames per seat |
| --- | --- | --- | ---: | ---: |
| Red | First legal | Red won by den entry | 139 | 140 |
| Blue | First legal | Blue won by den entry | 36 | 37 |
| Both | Self | Threefold repetition | 79 | 80 |

The separate `npm run conformance` workflow loaded the distributed component and fixture without rebuilding Rust. Its match UI completed the Red game and replayed revision 1 after displaying revision 139.

The release scored 52.625% over 200 color-swapped pairs against the previous search adapted to the v2 rules, with a paired 95% interval of 50.25–54.875%. [Strength report](hard-ai-benchmark.md)

The public DeepSeek v4.1 Flash · DeepSeek Harness 2.0.0 download passed the same admission checks. At 20,000 nodes, GPT-6 · Codex drew as Red after 136 plies on the quiet limit and lost as Blue after 181 plies by den entry. This single opening pair scored 25%. The record includes the opponent's SHA-256 and both move histories. [Match record](evidence/external-match.json) · [Opponent download site](https://deepseek-jungle.pages.dev/)

## Reproduce the checks

Run the following after installing the pinned dependencies and browsers as described in the README and CI workflow.

```sh
npm run verify:release
npm run build
npm run verify:python
npm run test:python
npm run lint:rust
npm run test:rust
npm run lint
npm test
npm run test:release
npm run test:controls
node scripts/browser-release.mjs --target all --browsers chrome,edge,firefox,webkit
node scripts/browser-release.mjs --target hard --browsers chrome
node scripts/browser-release.mjs --target human-hard --browsers chrome
node scripts/browser-release.mjs --target fallback-game --browsers chrome
```

The Python provenance check verified 18 frozen source files. All 110 Python tests, the Rust workspace tests, 15 Vitest tests, Clippy, ESLint, TypeScript and the production build passed. Browser controls covered search cancellation, undo and redo, Blue first, board flip, mute, auto-play pause, version-1 save rejection and a version-2 round trip. [Controls record](evidence/controls.json)

## Browser games

Each browser finished a scripted human game on the local production server and from the release HTML opened through a file URL. The offline tests blocked HTTP and HTTPS. Every download from the setup screen had the release filename and SHA-256.

| Browser | Hosted cold start | File cold start | Hosted / file plies | Normal runtime |
| --- | ---: | ---: | ---: | --- |
| Chrome 158.0.8127.38 | 642 ms | 315 ms | 162 / 68 | Worker |
| Edge 158.0.4370.33 | 643 ms | 378 ms | 68 / 74 | Worker |
| Firefox 153.0 | 759 ms | 326 ms | 208 / 156 | Worker |
| Playwright WebKit 26.5 | 627 ms | 350 ms | 76 / 76 | Worker |

The offline Firefox game drew by repetition. The other seven games ended by den entry. Disabling worker construction in all four browsers selected the main-thread fallback and produced a legal Medium opening reply. Native Safari was unavailable on Windows. [Browser measurements and outcomes](evidence/browser-all-chrome-edge-firefox-webkit.json)

The screenshot review covered setup, loading, help, selection, history, diagnostics, results and the two Arena orientations. [Screenshots](evidence/screens/)

In Chrome, Hard AI-vs-AI drew by repetition after 297 plies. Mean request-to-UI time was 963.7 ms and the maximum was 973.7 ms. In the human-vs-Hard game, Blue won by den entry after 94 plies. Its 47 replies averaged 895.4 ms and peaked at 970.1 ms. The separate forced-fallback game played 165 plies to a quiet-limit draw with the network blocked. [AI-vs-AI](evidence/browser-hard-chrome.json) · [Human vs Hard](evidence/browser-human-hard-chrome.json) · [Full fallback game](evidence/browser-fallback-game-chrome.json)

## Public release

Cloudflare Pages deployed the files built from commit `8b55a58` to the existing `gpt-jungle` project on its production branch `main` on November 24, 2026. The component, offline HTML, host module, compiler Wasm and published checksum manifest returned HTTP 200 with matching bytes at the production alias and immutable URL. Both Wasm downloads used `application/wasm`. GitHub's raw release files also matched both checksums. [Production](https://gpt-jungle.pages.dev/) · [Immutable deployment](https://f6cc46a1.gpt-jungle.pages.dev/) · [Release source](https://github.com/xmiao76/gpt_jungle_web/commit/8b55a58)

A fresh Windows clone verified the committed files before installing dependencies. Running `npm ci` and `npm run build` in that clone reproduced both release SHA-256 values. The complete Linux CI suite passed on the release commit. [CI run](https://github.com/xmiao76/gpt_jungle_web/actions/runs/45485567172)

`npm run test:public` completed games on the production alias in Chrome, Edge, Firefox and WebKit. Each ended by den entry. Chrome also completed a game on the immutable deployment. At each URL the test selected the bytes downloaded through the game menu in the conformance host, won both fixture matches and reached the same repetition draw in self-play. [Production results](evidence/browser-public-chrome-edge-firefox-webkit.json) · [Immutable deployment results](evidence/browser-public-chrome.json)

Chrome opened the public Arena upload route on November 24, 2026. It returned HTTP 200 with the leaderboard homepage, canonical URL `https://jungle-leaderboard.pages.dev/`, zero file inputs and no Arena or upload link. Catalog admission and ranking remain unverified. The deployed independent host accepts the downloadable entrant. [Public Arena upload route](https://jungle-leaderboard.pages.dev/arena/upload/) · [Deployed conformance host](https://gpt-jungle.pages.dev/conformance/)
