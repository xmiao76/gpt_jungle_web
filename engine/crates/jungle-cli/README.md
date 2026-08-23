# jungle-cli

`jungle-cli` is the native rules referee, search adapter, and champion arena.
With no arguments (or `serve`) it reads one JSON command per stdin line and
writes exactly one JSON response per stdout line. Diagnostic logging belongs on
stderr because non-JSON stdout is a protocol violation.

Every command carries an unsigned `id`; the response repeats it. The supported
commands are:

```json
{"type":"hello","id":1}
{"type":"newGame","id":2,"seed":0}
{"type":"chooseMove","id":4,"limits":{"kind":"fixedNodes","nodes":2000}}
{"type":"chooseMove","id":5,"limits":{"kind":"exactTime","timeMs":1800}}
{"type":"applyMove","id":6,"move":{"origin":42,"destination":35}}
{"type":"close","id":7}
```

`setPosition` uses the same validated `GameStateDto` object as save files; its
63-square payload is omitted from the compact example above.

`chooseMove` does not mutate the game. `applyMove` is the only advancing
command, and coordinates are independently reconstructed and checked by
`jungle-core`. `newGame` and `setPosition` reset search state; transposition and
history data persist across moves within one game. Invalid positions, illegal
moves, node overruns, and deadline overruns are structured error responses.

The public arena always launches candidate and champion as JSONL subprocesses:

```powershell
jungle-cli arena `
  --candidate .\candidate.exe `
  --champion python --champion-arg frozen_champion.py `
  --corpus .\promotion-v1.json `
  --time-ms 1800 --require-promotion --pretty
```

Each opening is played twice with candidate colors swapped. Only canonical game
wins count as wins; a still-ongoing game after 256 plies is a benchmark-only
draw. Results include W/D/L overall and by color, all game records, a
deterministic paired 95% bootstrap interval, strict failures, and promotion
reasons. Promotion requires at least 200 complete pairs, a lower confidence
bound above 50%, at least 45% raw score with each color, and no protocol,
illegal-move, or deadline forfeits.

The bootstrap interval is the two-sided 95% interval (2.5th and 97.5th
percentiles); requiring its lower endpoint above 50% is deliberately stricter
than a one-sided 95% gate. A forfeit by either candidate or champion increments
`strictFailures` and invalidates promotion, so a broken champion cannot become
evidence of candidate strength.

Omitting `--corpus` or passing `--smoke` uses the embedded initial-position
smoke corpus. `--candidate self --champion self` resolves both players to the
current executable for a convenient local self-play check.
