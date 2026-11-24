# Architecture

The release has two Wasm targets. The standalone wasm-bindgen target exports opening, legal move generation, checked commits, fixed-node search and wall-clock search to React. The Arena wit-bindgen target exposes only the four functions in the game interface. Both use the same Rust position, adjudication, principal-variation search, and evaluation.

A match snapshot contains the starting and current side, a compact board, canonical plies, quiet count, revision, outcome and reason. To recover repetition context at a fresh Arena instance, reverse the quiet tail of the supplied history on its present board and replay that tail. A capture reduces the piece count, so positions earlier than that capture cannot recur. Search make/unmake restores quiet counts, repetition history and the transposition context.

The shell derives legal highlights from Rust. Each AI request has a generation identity, an absolute deadline and a snapshot. Undo, new game, load and pause cancel outstanding work. The renderer uses canonical indices at every orientation and keeps matching animals keyed by owner and kind. Arena frames use build-time resampled copies of the same artwork plus embedded bitmap lettering.

The release build verifies the canonical rules digest, assembles both Wasm targets, bakes the component and engine into the single-file edition, and compares the component’s complete decoded WIT with a reference component built from the checked-in WIT. A local browser host transpiles selected bytes at runtime and runs both seats against an independently implemented referee.
