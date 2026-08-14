# Hard AI benchmark

## Scope

`web-hard-v2` is an isolated browser-only Hard-search overlay. It does not modify the manifest-locked desktop source under `python/jungle`, and Easy/Medium still use that source directly. Hard keeps the desktop rule engine, `SearchConfig.hard()`, Zobrist hashing, fixed-size transposition table, bounded quiescence, repetition guidance, and exact two-piece tablebase.

The overlay improves fixed-time search work through:

- bitboard-driven, allocation-light legal move generation with exact upstream ordering;
- precomputed terrain and neighbor data plus integer masks in evaluation;
- direct packed-move handling in hot paths;
- lower-overhead batched wall-clock checks while retaining exact node limits;
- session-scoped transposition storage whose prior-generation entries are used only as safe move-ordering hints;
- stable den, capture, trap, jump, killer, and history ordering.

Random reachable-position tests compare generated moves, make/unmake state, winner state, piece data, and incremental hashes with the locked desktop compact core. The tactical corpus also covers den entry/defense, traps, Rat/Elephant rules, both jump spans and blockers, poisoned captures, den races, and exact tablebase positions.

## Reproduce

```powershell
npm run benchmark:ai -- --samples 3
```

The command fails unless all 20 tactical/endgame positions pass, median browser-overlay throughput is at least 1.20 times the locked legacy Hard search, and the 1,800 ms overlay search completes at least the depth reached by legacy Hard.

## Recorded result

Measured 2026-08-13 on:

- AMD Ryzen 5 PRO 2400G with Radeon Vega Graphics
- Windows 10 Pro 10.0.19045
- CPython 3.12.9
- three 400 ms performance samples and one 1,800 ms comparison per engine

| Gate | Locked legacy Hard | `web-hard-v2` | Result |
| --- | ---: | ---: | --- |
| Tactical/endgame corpus | — | 20/20 | Pass |
| Median nodes/second | 14,347 | 19,217 | 1.339× |
| Completed depth at 1,800 ms | 5 | 6 | Pass |
| Overlay elapsed time | — | 1,801 ms | Within browser envelope |

A real Pyodide/Worker run in Playwright Chromium 151.0.7922.34 completed the initial-position Hard search at depth 5 with 21,312 nodes in 1,801.5 ms. During that search the main thread sampled 52.0 animation frames/second and opened the How to Play dialog in 82 ms; the automated acceptance floors are 30 frames/second and 750 ms interaction latency.

These measurements establish a focused quality improvement: the overlay preserves the tactical suite and searches one full iterative-deepening level farther than the legacy implementation on the recorded host under the same budget. They are not an Elo claim; wall-clock node counts and completed depth vary with hardware and browser load.
