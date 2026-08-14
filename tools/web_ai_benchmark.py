from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
from statistics import median
import sys


ROOT = Path(__file__).resolve().parents[1]
PYTHON_ROOT = ROOT / "python"
if str(PYTHON_ROOT) not in sys.path:
    sys.path.insert(0, str(PYTHON_ROOT))

from jungle.ai import AlphaBetaAI, SearchConfig
from jungle.domain import GameState, Piece, PieceType, Position, Side, initial_state
from web_ai import BrowserHardAI


TACTICAL_TIME_MS = 80
PERFORMANCE_TIME_MS = 400
HARD_TIME_MS = 1_800
MIN_NPS_RATIO = 1.20


@dataclass(frozen=True, slots=True)
class BenchmarkCase:
    name: str
    state: GameState
    destinations: frozenset[int]
    origins: frozenset[int] = frozenset()

    def accepts(self, move) -> bool:
        return bool(
            move is not None
            and move.destination in self.destinations
            and (not self.origins or move.origin in self.origins)
        )


def square(row: int, column: int) -> int:
    return Position(row, column).index


def piece(side: Side, kind: PieceType) -> Piece:
    return Piece(side, kind)


def state(items: dict[int, Piece], side: Side = Side.BLUE) -> GameState:
    board = [None] * 63
    for index, animal in items.items():
        board[index] = animal
    return GameState(board, side)


def tactical_cases() -> list[BenchmarkCase]:
    blue = Side.BLUE
    red = Side.RED
    return [
        BenchmarkCase(
            "immediate_den_entry",
            state({square(1, 3): piece(blue, PieceType.CAT), square(8, 6): piece(red, PieceType.RAT)}),
            frozenset({square(0, 3)}),
        ),
        BenchmarkCase(
            "block_den_entry",
            state({
                square(7, 3): piece(red, PieceType.CAT),
                square(7, 2): piece(blue, PieceType.DOG),
                square(0, 0): piece(red, PieceType.LION),
            }),
            frozenset({square(7, 3)}),
        ),
        BenchmarkCase(
            "trap_capture",
            state({
                square(7, 3): piece(red, PieceType.ELEPHANT),
                square(7, 2): piece(blue, PieceType.RAT),
                square(0, 0): piece(red, PieceType.LION),
            }),
            frozenset({square(7, 3)}),
        ),
        BenchmarkCase(
            "rat_takes_elephant",
            state({
                square(2, 0): piece(blue, PieceType.RAT),
                square(2, 1): piece(red, PieceType.ELEPHANT),
                square(8, 6): piece(red, PieceType.LION),
            }),
            frozenset({square(2, 1)}),
        ),
        BenchmarkCase(
            "rat_wins_den_race_from_right",
            state({
                square(2, 2): piece(blue, PieceType.RAT),
                square(2, 1): piece(red, PieceType.ELEPHANT),
                square(8, 6): piece(red, PieceType.LION),
            }),
            frozenset({square(1, 2)}),
        ),
        BenchmarkCase(
            "rat_wins_den_race_from_above",
            state({
                square(1, 1): piece(blue, PieceType.RAT),
                square(2, 1): piece(red, PieceType.ELEPHANT),
                square(8, 6): piece(red, PieceType.LION),
            }),
            frozenset({square(1, 2)}),
        ),
        BenchmarkCase(
            "lion_uses_three_square_span",
            state({square(3, 0): piece(blue, PieceType.LION), square(8, 6): piece(red, PieceType.RAT)}),
            frozenset({square(3, 3)}),
        ),
        BenchmarkCase(
            "lion_avoids_unsafe_empty_jump",
            state({
                square(3, 0): piece(blue, PieceType.LION),
                square(6, 6): piece(blue, PieceType.RAT),
                square(2, 3): piece(red, PieceType.LION),
                square(8, 6): piece(red, PieceType.RAT),
            }),
            frozenset({square(4, 0), square(2, 0), square(7, 6), square(5, 6), square(6, 5)}),
        ),
        BenchmarkCase(
            "tiger_uses_two_square_span",
            state({square(3, 0): piece(blue, PieceType.TIGER), square(8, 6): piece(red, PieceType.RAT)}),
            frozenset({square(3, 3)}),
        ),
        BenchmarkCase(
            "tiger_avoids_three_square_span",
            state({square(2, 1): piece(blue, PieceType.TIGER), square(8, 6): piece(red, PieceType.LION)}),
            frozenset({square(2, 0), square(2, 2), square(1, 1)}),
        ),
        BenchmarkCase(
            "den_race_pressure",
            state({
                square(2, 3): piece(blue, PieceType.CAT),
                square(7, 6): piece(red, PieceType.ELEPHANT),
                square(8, 0): piece(blue, PieceType.RAT),
            }),
            frozenset({square(1, 3)}),
        ),
        BenchmarkCase(
            "red_immediate_den_entry",
            state({square(7, 3): piece(red, PieceType.CAT), square(0, 6): piece(blue, PieceType.RAT)}, red),
            frozenset({square(8, 3)}),
        ),
        BenchmarkCase(
            "red_blocks_den_entry",
            state({
                square(1, 3): piece(blue, PieceType.CAT),
                square(1, 2): piece(red, PieceType.DOG),
                square(8, 6): piece(blue, PieceType.LION),
            }, red),
            frozenset({square(1, 3)}),
        ),
        BenchmarkCase(
            "lion_jump_capture",
            state({
                square(3, 0): piece(blue, PieceType.LION),
                square(3, 3): piece(red, PieceType.TIGER),
                square(8, 6): piece(red, PieceType.RAT),
            }),
            frozenset({square(3, 3)}),
        ),
        BenchmarkCase(
            "tiger_vertical_jump_capture",
            state({
                square(3, 0): piece(blue, PieceType.TIGER),
                square(3, 3): piece(red, PieceType.LEOPARD),
                square(8, 6): piece(red, PieceType.RAT),
            }),
            frozenset({square(3, 3)}),
        ),
        BenchmarkCase(
            "rat_blocks_lion_jump",
            state({
                square(3, 0): piece(blue, PieceType.LION),
                square(3, 1): piece(red, PieceType.RAT),
                square(8, 6): piece(red, PieceType.CAT),
            }),
            frozenset({square(2, 0), square(4, 0)}),
        ),
        BenchmarkCase(
            "avoid_poisoned_cat_capture",
            state({
                square(2, 2): piece(blue, PieceType.DOG),
                square(2, 3): piece(red, PieceType.CAT),
                square(2, 4): piece(red, PieceType.ELEPHANT),
                square(6, 6): piece(blue, PieceType.RAT),
            }),
            frozenset({square(1, 2)}),
            frozenset({square(2, 2)}),
        ),
        BenchmarkCase(
            "prefer_den_entry_over_material",
            state({
                square(1, 3): piece(blue, PieceType.CAT),
                square(2, 0): piece(blue, PieceType.RAT),
                square(2, 1): piece(red, PieceType.ELEPHANT),
                square(8, 6): piece(red, PieceType.LION),
            }),
            frozenset({square(0, 3)}),
            frozenset({square(1, 3)}),
        ),
        BenchmarkCase(
            "exact_cat_dog_endgame",
            state({square(4, 0): piece(blue, PieceType.CAT), square(4, 6): piece(red, PieceType.DOG)}),
            frozenset({square(3, 0)}),
        ),
        BenchmarkCase(
            "exact_lion_elephant_endgame",
            state({square(6, 0): piece(blue, PieceType.LION), square(2, 6): piece(red, PieceType.ELEPHANT)}),
            frozenset({square(6, 1)}),
        ),
    ]


def run_tactics(time_ms: int = TACTICAL_TIME_MS) -> int:
    passed = 0
    print(f"tactical corpus time_ms={time_ms}")
    for case in tactical_cases():
        result = BrowserHardAI().choose_move(case.state, time_limit_ms=time_ms)
        ok = case.accepts(result.move)
        passed += int(ok)
        move = "none" if result.move is None else f"{result.move.origin}->{result.move.destination}"
        print(
            f"  {case.name}: {'PASS' if ok else 'FAIL'} move={move} "
            f"depth={result.depth} nodes={result.nodes} elapsed_ms={result.elapsed_ms:.0f}"
        )
    return passed


def run_performance(samples: int = 3) -> tuple[float, int, int]:
    reference = initial_state()
    legacy_nps: list[int] = []
    browser_nps: list[int] = []
    for _ in range(samples):
        legacy = AlphaBetaAI(PERFORMANCE_TIME_MS, SearchConfig.hard()).choose_move(reference.copy())
        browser = BrowserHardAI().choose_move(reference.copy(), PERFORMANCE_TIME_MS)
        legacy_nps.append(legacy.nodes_per_second)
        browser_nps.append(browser.nodes_per_second)

    legacy_full = AlphaBetaAI(HARD_TIME_MS, SearchConfig.hard()).choose_move(reference.copy())
    browser_full = BrowserHardAI().choose_move(reference.copy(), HARD_TIME_MS)
    ratio = median(browser_nps) / max(1, median(legacy_nps))
    print(
        "performance "
        f"legacy_median_nps={median(legacy_nps):.0f} browser_median_nps={median(browser_nps):.0f} "
        f"ratio={ratio:.3f} legacy_depth={legacy_full.depth} browser_depth={browser_full.depth} "
        f"browser_elapsed_ms={browser_full.elapsed_ms:.0f}"
    )
    return ratio, legacy_full.depth, browser_full.depth


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description="Validate the browser-only Jungle Hard AI.")
    parser.add_argument("--samples", type=int, default=3, help="short performance samples (default: 3)")
    args = parser.parse_args(argv)
    if args.samples <= 0:
        parser.error("--samples must be positive")

    passed = run_tactics()
    ratio, legacy_depth, browser_depth = run_performance(args.samples)
    failures: list[str] = []
    if passed != len(tactical_cases()):
        failures.append(f"tactical score {passed}/{len(tactical_cases())}")
    if ratio < MIN_NPS_RATIO:
        failures.append(f"NPS ratio {ratio:.3f} below {MIN_NPS_RATIO:.2f}")
    if browser_depth < legacy_depth:
        failures.append(f"browser depth {browser_depth} below legacy depth {legacy_depth}")
    if failures:
        raise SystemExit("AI benchmark failed: " + "; ".join(failures))
    print(f"PASS tactical={passed}/{len(tactical_cases())} nps_ratio={ratio:.3f}")


if __name__ == "__main__":
    main()
