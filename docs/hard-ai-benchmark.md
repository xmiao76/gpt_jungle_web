# Release strength comparison

The release scored **52.625%** across 400 complete games against the search and evaluation from commit `72ca198`. The two-sided 95% paired bootstrap interval was **50.25% to 54.875%**. Red scored 48 wins, 114 draws and 38 losses. Blue scored 51 wins, 109 draws and 40 losses.

| Hard node budget | Opening pairs | Candidate score | Paired 95% interval |
| --- | ---: | ---: | ---: |
| 1,000 | 100 | 54.50% | 51.75%–57.25% |
| 5,000 | 100 | 50.75% | 47.00%–54.50% |

Each deterministic opening was played with the same seed and budget twice, with colors exchanged. A win scores one point and a draw half a point. The combined interval resamples whole opening pairs 10,000 times. The 5,000-node result alone is inconclusive.

The comparison runs the older search on the same v2 board and under the same referee. The adapter accounts for simulated defensive replies and maps the additional draw result. Both engines use the shipped classical evaluation. The candidate incorporates the game’s reversible history and quiet counter into its search decisions. The frozen historical tablebase is absent from both players.

Run `npm run benchmark:ai` in a clone containing commit `72ca198`. The runner builds the pinned historical source into an ignored build directory. All game results, seeds, opening plies, timings and interval parameters are in [the strength report](evidence/strength.json).
