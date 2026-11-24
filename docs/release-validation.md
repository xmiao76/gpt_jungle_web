# Jungle 2.0.0 release validation

The release identifies itself as **GPT-6 · Codex**, platform `official`. Validation used Windows 10, Rust 1.94.1, wasm-tools 1.263.0, wasm-bindgen-cli 0.2.130, Node 26.13.1 and jco-transpile 0.24.0.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `release/gpt-6-codex-2.0.0.wasm` | 452,804 | `4cafc7f31b48503660da34fc5c1e62a50605c91249b70067baca091de7378034` |
| `release/index.html` | 1,241,268 | `375a68e2a1a761d5cfcb82d07870ec4df0a504b39376404e9a560bda168c3146` |

## Rules, strategy and component

The release build checked the SHA-256 of the canonical rules profile against `a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57`. The independent referee and Rust engine agreed on both openings, all 23 portable legality cases, capture and quiet counters, outcome precedence and repetition.

The component's expanded WIT matched the checked-in world. Transpilation took 579 ms. The fresh-instance admission smoke test took 133 ms. On the opening, Hard used 45 ms at 20,000 nodes and 1.51 s at 500,000 nodes. Geometry, opacity, flip, deterministic choice, isolated instances, capture animation and direct replay were checked against the release file. [Admission measurements](evidence/admission.json)

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
