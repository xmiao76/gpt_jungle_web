# Jungle

Play Dou Shou Qi on the [hosted site](https://gpt-jungle.pages.dev/) or open [the standalone HTML](release/index.html) from a checkout. The local edition includes all art, styles, code, and engine bytes. Choose the first color, first mover, difficulty and game mode in the setup dialog.

<!-- release -->

GPT-6 · Codex · 2.0.0

- Model GPT-6, code agent Codex, platform official.
- Engine 2.0.0, Rust / WebAssembly.
- Source https://github.com/xmiao76/gpt_jungle_web
- Rules `desktop-jungle-v2` · `a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57`
- `gpt-6-codex-2.0.0.wasm` · SHA-256 `4cafc7f31b48503660da34fc5c1e62a50605c91249b70067baca091de7378034`
- `release/index.html` · SHA-256 `375a68e2a1a761d5cfcb82d07870ec4df0a504b39376404e9a560bda168c3146`

<!-- /release -->

## Playing

Select a friendly animal and then a highlighted destination. Dots indicate moves and rings indicate captures. Arrow keys move focus around the board. Flip rotates both presentation and input. Move history always uses the original square numbers. In human games Undo returns to the previous decision. In automatic games it rewinds one ply and pauses. Redo restores the move. Portable version-2 JSON saves replay from their chosen opening. Version-1 saves belong to the earlier variant.

Enter the enemy den, take the last opposing piece or leave the opponent without a legal move to win. A third occurrence of the same board and turn draws the game. A hundred quiet plies also draw. Rats swim and can capture elephants on land. Captures cannot cross a shoreline. Lions jump both river spans, tigers only the short span, and either side’s swimming rat blocks a jump. An enemy trap sets a piece’s rank to zero before the capture comparison. The in-game rules cover every rank and terrain.

## Arena

Select **Download Arena .wasm** in the game, then choose the downloaded file on the [Arena upload page](https://jungle-leaderboard.pages.dev/arena/upload/). Wait for admission and choose **Play in left seat** or **Play in right seat**. With Node installed, run `npm ci` and `npm run conformance` to test the committed component locally without rebuilding Rust. The [browser conformance host](https://gpt-jungle.pages.dev/conformance/) also runs the release against the included first-legal opponent in either seat, runs self-play, and replays committed positions.

## Build and verify

Use Node 22.16 or newer, the repository’s pinned Rust toolchain and wasm32-unknown-unknown target, wasm-tools 1.263.0, and wasm-bindgen-cli 0.2.130. A release build uses relative asset URLs. The requirements and development milestones are in [jungle_prompt.md](jungle_prompt.md).

```sh
npm ci
cargo install --locked wasm-tools --version 1.263.0
cargo install --locked wasm-bindgen-cli --version 0.2.130
npm run build
npm run dev
npm run preview
```

The release command writes the static site to `dist`, the component and local HTML to `release`, and both SHA-256 values to this README and `release/SHA256.txt`. The HTML embeds the very same standalone engine as the hosted UI. Opening it through a file URL requires no installation or network.

The public download and offline edition are also available directly. [Arena component](https://gpt-jungle.pages.dev/gpt-6-codex-2.0.0.wasm) · [Single-file HTML](https://gpt-jungle.pages.dev/release/) · [Release source branch](https://github.com/xmiao76/gpt_jungle_web/tree/Arena-WebAssembly)

```sh
npm test
npm run test:rust
npm run test:release
npm run test:e2e
npm run benchmark:ai
wasm-tools component wit release/gpt-6-codex-2.0.0.wasm
npm run deploy
npm run test:public
```

## Runtime

Rust owns move legality, position updates, terminal outcomes and search. The React shell maintains the undo timeline, user input, audio and animations. Human games search for up to 250 ms on Easy, 750 ms on Medium and one second on Hard. Search runs in a warmed worker. Browsers that block worker construction use short searches over individual root moves with yields between them. The Arena build uses the same engine with deterministic node limits. Easy caps the supplied budget at 1,000 and Medium at 20,000. Hard uses the host budget. Quiescence, roots, revisits, and transposition hits count toward the ceiling.

The component implements [the versioned WIT](wit/jungle.wit), embeds its artwork, and provides its own 448 × 576 RGBA renderer. The host instantiates seats in separate workers. A separate JavaScript referee owns its official position. The legacy two-piece tablebase belongs to the earlier ruleset. Release checks and browser coverage are recorded in [release validation](docs/release-validation.md). See [art credits](docs/art-license.txt).
