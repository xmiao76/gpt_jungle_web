# Prompt: GPT_CX_Jungle Web Edition

Act as a senior software architect, game engineer, frontend engineer, and release owner. Plan, implement, test, and deploy a production-quality browser version of the existing Jungle / Dou Shou Qi desktop game.

Do not stop after producing a plan or mockup. Completion requires a working application, automated tests, documentation, and a verified public Cloudflare Pages deployment.

## Objective

Convert the proven Windows desktop project at:

`C:\GitHub\gpt_cx_jungle`

into a static web application in:

`C:\GitHub\gpt_jungle_web`

The web app must let a human play complete Jungle games against the existing Python AI on a polished visual board. All rules and AI computation run locally in the visitor's browser through Pyodide. Cloudflare only hosts static files; there is no server-side game logic, API, database, Pages Function, or paid Worker feature.

The initial source baseline is desktop commit:

`b0bb427458a686aa0ed10770bda50f9311a7845b`

At that baseline, `python -m pytest -q` passes 152 tests. Preserve that implementation as the behavioral reference.

## Authoritative sources

Use these sources in this order of authority:

1. The tested desktop implementation under `C:\GitHub\gpt_cx_jungle\src\jungle`.
2. `C:\GitHub\gpt_cx_jungle\docs\ruleset.md`.
3. The standard Jungle rules at <https://en.wikipedia.org/wiki/Jungle_(board_game)>.
4. The board reference at <https://veryspecial.us/free-downloads/AncientChess.com-DouShouQi.pdf> when layout or starting-position details are unclear.

Do not create a second rules implementation in TypeScript. If external sources describe a variant that differs from the tested engine, retain the engine behavior and document the choice.

## Source-code facts that must shape the port

The desktop project is not organized like other Jungle implementations. Use its real structure:

- `src/jungle/domain/`: board constants and serializable domain types.
- `src/jungle/rules/`: legal movement, captures, jumps, traps, and terminal evaluation.
- `src/jungle/engine/`: authoritative `Game`, move application, undo/redo, and state serialization.
- `src/jungle/ai/`: baseline, medium, and compact Hard alpha-beta search implementations.
- `src/jungle/ai/data/two_piece_v1.jgtb`: the approximately 1.0 MB exact two-piece tablebase used by Hard AI.
- `src/jungle/ui/assets/`: 23 existing board, terrain, den, trap, and animal PNG files totaling approximately 86 KB.
- `src/jungle/app/` and `src/jungle/ui/`: desktop-only Tkinter, threading, dialogs, and presentation code.

The reusable domain, rules, engine, and AI layers use only the Python standard library. Their use of dataclasses, enums, `array`, `hashlib`, `importlib.resources`, `pathlib`, `struct`, and `time.perf_counter` is compatible with the selected Pyodide runtime.

Reuse the following in the web repository with no semantic changes:

- `jungle/domain`
- `jungle/rules`
- `jungle/engine`
- `jungle/ai`
- `jungle/ai/data/two_piece_v1.jgtb`

Do not copy or import the Tkinter UI, desktop controller, PyInstaller specification, executable packaging code, or desktop release scripts into the browser runtime.

Add a source manifest recording the desktop repository URL, source commit, and hashes of all copied Python and tablebase files. Add a deterministic verification command that fails if the committed browser payload no longer matches its manifest. When deliberately syncing a newer desktop revision, update the copied sources, relevant tests, manifest, and documented source commit together.

## Rules that must remain exact

The web app must behave exactly like `jungle.rules` and `jungle.engine`:

- The board is 7 columns by 9 rows.
- Blue always moves first.
- Entering the opponent den wins.
- Capturing all opposing pieces wins.
- A side with no legal moves loses.
- There is no gameplay no-capture draw or threefold-repetition terminal rule. Repetition handling in the source project is AI guidance and benchmark adjudication only.
- A piece cannot enter its own den.
- Only rats enter water.
- A rat in water can capture only another rat in water; land/water boundary captures are forbidden.
- A land rat can capture an elephant.
- An elephant can capture a rat only when that rat has effective rank zero in the elephant's own trap.
- An enemy piece in a trap has effective rank zero.
- A lion can jump either river span: across 2 water squares vertically or 3 water squares horizontally.
- A tiger can jump only across the 2-water-square span.
- A rat of either side anywhere in the jump path blocks a lion or tiger jump.

Do not add a draw result merely because an automated benchmark uses a ply cap or repetition adjudication.

## Required technology stack

Use the following stack unless a verified platform defect makes one item impossible. Document any necessary deviation before implementing it.

- React with TypeScript for the application UI.
- Vite for development and production builds.
- Modern CSS for layout, responsive behavior, transitions, and theming.
- A DOM/CSS Grid board, not Canvas, with 63 accessible square buttons and a positioned piece/animation layer.
- A dedicated module Web Worker containing Pyodide, the authoritative game session, and AI search.
- Pyodide `314.0.3`, pinned exactly, using its Python 3.14 runtime and official jsDelivr distribution.
- Vitest and React Testing Library for TypeScript and component tests.
- Playwright for real-browser and Pyodide integration tests.
- Pytest under CPython 3.12 or newer for the copied Python domain, rules, engine, AI, and tablebase tests.
- Node.js 22, pinned through a repository version file, with an npm lockfile committed.

Relevant platform references:

- Pyodide Web Worker guidance: <https://pyodide.org/en/stable/usage/webworker.html>
- Pyodide bundler guidance: <https://pyodide.org/en/stable/usage/working-with-bundlers.html>
- Cloudflare Pages: <https://developers.cloudflare.com/pages/>
- Cloudflare Pages build configuration: <https://developers.cloudflare.com/pages/configuration/build-configuration/>

Configure Vite with relative asset paths so the built site works at a domain root or arbitrary static-host subpath. Do not introduce client-side routing when a single application page is sufficient.

## Python payload and Pyodide initialization

Store the committed reusable Python source under a clearly isolated browser-payload directory such as `python/jungle/`. Put the web-only Python adapter beside it, not inside the copied source packages.

Build a deterministic zip archive during `npm run build` using a Node-based script so the Cloudflare frontend build does not require building a Python wheel. The archive must contain:

- all required `__init__.py` files;
- the copied `domain`, `rules`, `engine`, and `ai` modules;
- the complete `jungle/ai/data/two_piece_v1.jgtb` asset;
- a small browser-session adapter.

The module Worker must:

1. Load the pinned Pyodide runtime from the official CDN.
2. Fetch the same-origin, build-hashed Python archive.
3. Extract it into Pyodide's virtual filesystem.
4. Add the extraction root to `sys.path`.
5. Import the browser adapter and create one authoritative session.
6. Confirm that the tablebase loads successfully before reporting full readiness.
7. Return a typed initial snapshot to the main thread.

Preserve the package layout after extraction so `importlib.resources.files("jungle.ai").joinpath("data", "two_piece_v1.jgtb")` works without modifying the tablebase implementation.

Show staged loading feedback for at least: Pyodide runtime, Python engine, tablebase/session initialization, and ready. Provide a Retry action after initialization failure and show a useful error without leaving a fake playable board.

## Authoritative worker protocol

Define the protocol as TypeScript discriminated unions plus runtime guards. Every request and response must carry a request ID and session revision so stale results can be rejected.

Use an envelope equivalent to:

```ts
type WorkerRequest = {
  id: number;
  revision: number;
  type:
    | "init"
    | "newGame"
    | "legalTargets"
    | "applyMove"
    | "aiMove"
    | "undo"
    | "redo"
    | "importGame"
    | "exportGame";
  payload?: unknown;
};

type WorkerResponse = {
  id: number;
  revision: number;
  type: "progress" | "ready" | "state" | "thinking" | "export" | "error";
  payload: unknown;
};
```

Lock down the payload for each message with TypeScript types and validation. At minimum:

- `newGame` receives difficulty and the selected human side.
- `legalTargets` receives a canonical origin index and returns authoritative legal destination indices with capture/jump metadata.
- `applyMove` receives canonical origin and destination indices and returns the new state or a structured illegal-move error.
- `aiMove` receives a difficulty and returns the applied move, new state, and search diagnostics.
- `undo` and `redo` receive a positive ply count and return the resulting state plus `canUndo` and `canRedo`.
- `importGame` validates a versioned save before replacing the current session.
- `exportGame` returns the current serializable state.

State snapshots must include `GameState.to_dict()` data, move-history display data, the last move, terminal status, `canUndo`, `canRedo`, and optional AI diagnostics. The main thread may cache the latest snapshot for rendering and crash recovery, but it must not become authoritative.

The main thread owns only presentation concerns: selected square, animations, dialogs, responsive panels, audio, board orientation, and the scheduling loop for AI-vs-AI. It must never calculate legal moves, captures, ranks, jumps, or winners.

Python search is synchronous inside the Worker, so state-changing controls must be disabled while a move is being searched. An AI-vs-AI Stop request takes effect after the current search completes. Ignore responses whose request ID or revision is no longer current.

## Game-session behavior

Match and improve the desktop interaction model without changing game rules:

- Blue is always the engine's initial side to move.
- "Human starts" assigns Blue to the human and Red to the AI.
- "AI starts" assigns Blue to the AI and Red to the human.
- Starting-side assignment is independent of board orientation.
- Standard play accepts input only for the configured human side and only on that side's turn.
- AI-vs-AI repeatedly requests one move at a time, yielding to the UI between moves so it can stop cleanly.
- Undo in human-vs-AI should normally return to the previous human decision point by undoing the completed human/AI pair. If only one ply exists, undo one ply.
- Redo must restore the exact plies removed by the preceding undo rather than asking the AI to choose a different replacement move.
- AI-vs-AI undo/redo operates one ply at a time.
- Importing a save starts with empty undo/redo stacks, matching the desktop save format's limitations.
- Flip Board changes only canonical-index-to-display mapping. It must not mutate state, swap sides, change the turn, alter the save, or trigger AI.

## Save and load

Implement portable browser file save/load using a Blob download and an ordinary file input so it works in Chrome, Edge, Firefox, and Safari without relying on the File System Access API.

Use a versioned wrapper equivalent to:

```json
{
  "schema": "gpt-jungle-save",
  "version": 1,
  "engineVersion": "0.1.0",
  "createdAt": "ISO-8601 timestamp",
  "gameState": {},
  "settings": {
    "difficulty": "medium",
    "humanSide": "blue",
    "boardFlipped": false
  }
}
```

The Python adapter must validate the candidate into a temporary session before replacing the live session. Reject malformed JSON, unsupported versions, invalid board length, invalid enums, or inconsistent move data with a clear UI error, leaving the current game untouched.

Store purely local preferences such as mute, reduced audio volume, and the How to Play dismissal state in `localStorage`. Do not require an account or remote storage.

## AI behavior and performance

Keep the existing AI profiles and search code:

- Easy uses `SearchConfig.baseline()`.
- Medium uses the existing desktop medium configuration.
- Hard uses `SearchConfig.hard()`, the compact make/unmake core, incremental Zobrist hashing, fixed-size transposition table, bounded quiescence, and exact two-piece tablebase.

Use these initial browser time budgets:

- Easy: 250 ms
- Medium: 750 ms
- Hard: 1,800 ms

Do not multiply these limits merely because Pyodide executes fewer nodes per second. They are user-facing response budgets. Benchmark and tune ordering or browser integration if necessary while preserving AI legality and profile distinctions.

Acceptance targets after the Worker is ready:

- The UI remains interactive at 60 fps while AI search runs.
- A normal Hard move returns in approximately 2.2 seconds or less, allowing modest messaging overhead beyond the 1.8-second search budget.
- Every AI result is legal for the exact state revision it searched.
- The tablebase loads once per Worker and produces a nonzero tablebase-hit count on a known two-piece position.
- Diagnostics show difficulty, completed depth, nodes, nodes per second, transposition hits, tablebase hits, and elapsed milliseconds.

Do not impose a fixed minimum search depth across all browsers and devices. Record benchmark hardware, browser, positions, timings, depths, and node counts in development documentation.

## UI and visual requirements

The final UI must look like a finished public game, not a developer demo.

Use a responsive 7-by-9 DOM board. Render terrain as cell backgrounds and pieces in a separate positioned layer so moves can animate cleanly. Never flip the board with a CSS rotation that turns labels and art upside down.

Canonical orientation shows Red at the top and Blue at the bottom. For a displayed cell index `v`, flipped mode maps to canonical index `62 - v`; unflipped mode maps to `v`. Apply the same mapping to rendering, pointer input, touch input, keyboard focus, highlights, and animation endpoints.

Reuse all suitable PNG artwork from `gpt_cx_jungle/src/jungle/ui/assets`. Each animal must remain recognizable and also display its full short English name, such as `Rat`, `Leopard`, or `Elephant`, in readable HTML text that follows the piece during animation. Provide useful `alt` text and square `aria-label` values.

Required UI features:

- A loading screen with staged progress and retry/error states.
- A start/new-game dialog with Easy, Medium, and Hard choices and Human Starts / AI Starts choices.
- Clear current-turn, controlled-side, AI-side, thinking, and terminal status.
- Two-step click/touch input: select a piece, then select a legal destination.
- Selected-piece highlight, distinct quiet-move and capture indicators, last-move highlight, capture feedback, and smooth move animation.
- Move history with side, animal name, coordinates, capture, and jump notation.
- Undo, Redo, Save, Load, Flip Board, Diagnostics, AI vs AI Start/Stop, New Game, and Mute controls.
- A diagnostics panel that is off by default and can show selected index, legal targets, effective rank, move type, and AI statistics.
- A polished result dialog for win/loss reason with New Game and review-board actions.
- Desktop side-panel layout, tablet adaptation, and a mobile layout that keeps the complete board and primary controls usable without horizontal scrolling.
- Keyboard-operable controls, visible focus states, adequate color contrast, and `prefers-reduced-motion` support.

Use Web Audio to synthesize short move, capture, and victory cues because the source project contains no WAV files. Initialize audio only after a user gesture, fail silently if audio is unavailable, and honor the mute setting.

Add a complete How to Play panel for a first-time player. It must explain:

- den-entry, capture-all, and no-legal-moves win conditions;
- selection, movement, capture indicators, and controls;
- rank order and the land Rat-versus-Elephant exception;
- water and rat capture restrictions;
- lion and tiger jump differences and rat blocking;
- enemy trap rank reduction and the own-den restriction;
- Undo, Redo, Save, Load, Flip, Mute, Diagnostics, and AI-vs-AI behavior;
- that this engine does not use a no-capture or repetition draw rule.

## Provenance and runtime information

Display version/runtime information in the main menu or About panel and repeat it in the README.

Separate the provenance of the reused engine from the provenance of the web port:

- Source engine model: `GPT 5.6-Sol`
- Source engine reasoning effort: `Ultra`
- Source engine code agent: `Codex`
- Source engine version: `0.1.0`
- Source engine commit: `b0bb427458a686aa0ed10770bda50f9311a7845b`
- Web port model, reasoning effort when available, and code agent: record the actual values used to implement the web version; never invent or copy placeholder values.
- Frontend application version from `package.json`.
- Runtime: Pyodide `314.0.3` and the detected Python version.

## Automated testing requirements

Testing is part of implementation, not a final cleanup step. Every defect found during browser play, packaging, or deployment must receive a regression test when practical.

### Python and source-parity tests

- Bring over the relevant existing tests for `domain`, `rules`, `engine`, `ai`, compact search, transposition table, evaluation, tablebase, and full-game behavior.
- Do not bring over Tkinter layout tests, desktop-controller mocks, PyInstaller tests, or packaged-EXE tests unchanged; replace their intended coverage with web tests.
- Run the copied engine suite under CPython 3.12 or newer in CI.
- Verify the copied tablebase header, rules hash, payload checksum, and representative probes.
- Verify the source manifest and copied-file hashes.

### TypeScript and React tests

Use Vitest and React Testing Library to cover at least:

- canonical/display index mapping in both orientations for all 63 cells;
- flip invariance of state, ownership, turn, and selected canonical square;
- worker request/response runtime guards;
- request ID and revision handling, including stale AI responses;
- loading, ready, recoverable-error, fatal-error, and retry states;
- Human Starts / AI Starts side assignment;
- piece selection, legal-target display, capture styling, and disabled input while thinking;
- undo/redo ply behavior in human-vs-AI and AI-vs-AI;
- save wrapper generation and invalid-save rejection;
- AI-vs-AI scheduling and stop-after-current-move behavior;
- mute and reduced-motion preferences.

### Pyodide and browser integration tests

Run the real built Worker in Playwright rather than mocking Pyodide for integration coverage.

Required scenarios:

1. Load the application, observe loading UI, wait for Worker readiness, and verify the initial 16-piece state.
2. Request legal targets, make a known legal human move, and verify side-to-move, history, and board state.
3. Request an AI move and prove the returned/applied move was legal for the searched revision.
4. Export, mutate, import, and verify a state round trip.
5. Flip before and after moves and verify the canonical state is unchanged.
6. Start with the AI as Blue and confirm it moves without blocking the main thread.
7. Probe a known two-piece Hard position and assert a successful tablebase hit.
8. Run a deterministic, test-only node-limited AI-vs-AI game to a real `den_entry`, `capture_all`, or `no_legal_moves` engine result. A ply-cap adjudication is not a passing terminal result.
9. Import a near-terminal fixture, complete the winning move through the UI, and verify the result dialog.

Run core smoke coverage in Chromium, Firefox, and WebKit. The deterministic full-game soak may run in Chromium only if total cross-browser runtime would be excessive.

## Development and CI workflow

Provide npm scripts with these responsibilities:

- `npm run dev`: Vite development server.
- `npm run build:python`: verify the source manifest and build the deterministic Python archive.
- `npm run build`: build the Python archive, type-check TypeScript, and create the Vite production bundle.
- `npm run preview`: serve the production bundle locally.
- `npm run lint`: lint TypeScript, React, scripts, and configuration.
- `npm test`: run Vitest/React tests.
- `npm run test:python`: run pytest for the copied Python implementation.
- `npm run test:e2e`: build and run Playwright integration tests.
- `npm run test:all`: run manifest verification, Python tests, lint, TypeScript tests, production build, and Playwright tests.
- `npm run deploy`: deploy the already validated `dist` directory to the configured Pages project.

Commit a GitHub Actions workflow for pull requests and `main` that installs locked Node dependencies, installs the Python test dependencies, installs Playwright browsers, and runs `npm run test:all`. Keep deployment credentials out of the repository.

## Cloudflare Pages deployment

Deploy as a static Cloudflare Pages site on the free tier.

- Git repository: `https://github.com/xmiao76/gpt_jungle_web.git`
- Production branch: `main`
- Pages project name: `gpt-jungle`
- Public URL: `https://gpt-jungle.pages.dev`
- Build command: `npm run build`
- Build output directory: `dist`
- Node runtime: pinned Node 22
- Pages Functions: none
- Server-side Workers: none
- Bindings, secrets, databases, KV, Durable Objects, and R2: none

Use Cloudflare Pages Git integration for normal production and preview deployments. Also support a manual authenticated deployment with:

```powershell
npm run build
npx wrangler pages deploy dist --project-name gpt-jungle --branch main
```

Set long-lived immutable caching for build-hashed assets and the Python payload while keeping `index.html` revalidatable. Ensure the Pyodide CDN is permitted by any Content Security Policy added to the site.

After every production deployment, validate the actual public URL rather than only the local preview:

- confirm `index.html`, JavaScript, CSS, images, Python archive, Pyodide modules, WASM, and tablebase initialization load without 404, CORS, CSP, or MIME errors;
- confirm there are no uncaught console exceptions;
- test Human Starts and AI Starts;
- play or observe at least one complete game to an authentic engine terminal result;
- verify save/load, undo/redo, flip, audio/mute, diagnostics, AI-vs-AI, and responsive layout;
- record the deployment URL and verification date in the README or release-validation document.

Offline support and a service worker are not required. Normal CDN and browser caching is sufficient.

## Expected repository contents

The finished repository should contain, at minimum:

- React/TypeScript application code grouped by UI, game-session state, protocol, and Worker responsibilities;
- the copied Python payload and separate web adapter;
- the source provenance/hash manifest and deterministic archive builder;
- reused game artwork;
- Vitest, React Testing Library, pytest, and Playwright tests;
- Vite, TypeScript, lint, Playwright, and Cloudflare static-header configuration;
- locked Node and Python development metadata;
- GitHub Actions CI;
- `README.md`;
- this root `prompt.md`;
- optional focused architecture and release-validation documents.

Do not commit `node_modules`, local virtual environments, Playwright reports, secrets, Wrangler credentials, or generated deployment state. Commit the reproducible source inputs and lockfiles; generate `dist` during build.

## Phased implementation roadmap

### Phase 1: parity foundation

- Scaffold the locked React/TypeScript/Vite toolchain.
- Copy and manifest the reusable Python packages, tablebase, and PNG assets.
- Build/load the Python archive in a module Worker.
- Implement and test the typed protocol and browser adapter.
- Render a responsive board and complete human-vs-AI turns.
- Add all three difficulties and starter/side selection.

Exit criterion: a complete human-vs-AI game can be played locally without UI blocking or JavaScript rule logic.

### Phase 2: full desktop parity and polish

- Add undo/redo, versioned save/load, flip, diagnostics, AI-vs-AI, move history, terminal dialogs, and search statistics.
- Add animations, responsive mobile/tablet behavior, accessibility, How to Play, Web Audio, and provenance/runtime UI.
- Tune loading, failure recovery, and AI performance.

Exit criterion: every required desktop and selected web feature works locally with automated coverage.

### Phase 3: release hardening

- Run CPython, Vitest, component, Pyodide, cross-browser, and deterministic full-game tests.
- Fix defects and add regression coverage.
- Complete README and release-validation documentation.
- Deploy `gpt-jungle` to Cloudflare Pages and validate the public URL.

Exit criterion: CI is green and the production deployment passes the full post-deploy checklist.

## README requirements

The finished `README.md` must include:

- live demo URL and current deployment status;
- screenshots or a short visual overview;
- gameplay goal and controls;
- difficulty and starter-selection behavior;
- exact local install, development, build, test, preview, and deploy commands;
- architecture overview showing React main thread, module Worker, Pyodide, Python engine/AI, and tablebase;
- explanation of canonical versus flipped board orientation;
- save-file behavior and compatibility version;
- browser support and known limitations;
- source desktop repository, source commit, engine version, and ruleset link;
- engine provenance and actual web-port provenance as separate entries;
- Cloudflare Pages project-name derivation: `gpt_jungle_web` becomes `gpt-jungle` by removing the generic `_web` suffix, converting underscores to hyphens, and keeping the meaningful project words;
- public deployment verification date.

## Completion criteria

The task is complete only when all of the following are true:

- The application is a real React/TypeScript web port, not an embedded desktop screenshot or rewritten TypeScript rules engine.
- The authoritative Python domain, rules, engine, AI, and tablebase run in pinned Pyodide inside a module Web Worker.
- A human can complete games against Easy, Medium, and Hard AI without the UI freezing.
- Human Starts, AI Starts, AI-vs-AI, undo/redo, save/load, board flip, diagnostics, history, audio/mute, animations, and How to Play all work.
- The web game matches the tested desktop rule behavior, including the absence of a gameplay draw rule.
- The copied Python tests, source-manifest check, TypeScript tests, component tests, Pyodide smoke tests, cross-browser tests, and deterministic terminal full-game test all pass.
- Every discovered defect has been fixed and regression-tested when practical.
- The production build uses only relative/static assets and contains no server-side compute.
- The site is deployed at `https://gpt-jungle.pages.dev` and the public deployment has been verified through a complete game.
- `README.md` contains the required workflows, architecture, live URL, and accurate provenance.
- This `prompt.md` remains at the repository root and accurately describes the implemented result.

