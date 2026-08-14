from __future__ import annotations

from jungle.ai.core import (
    BLUE,
    EMPTY,
    LION,
    NEIGHBORS,
    OWN_DEN,
    OPPONENT_DEN,
    PIECE_TYPE_COUNT,
    RAT,
    RED,
    RIVER_JUMPS,
    TIGER,
    WATER_MASK,
)
from jungle.ai.fast_evaluation import FAST_TERMINAL_SCORE, TABLEBASE_SCORE
from jungle.domain import BLUE_TRAPS, RED_TRAPS

from .position import BrowserCompactPosition, piece_kind_fast, piece_side_fast


PIECE_VALUES = (0, 145, 235, 320, 405, 510, 755, 825, 900)
TRAP_MASKS = (
    sum(1 << square for square in BLUE_TRAPS),
    sum(1 << square for square in RED_TRAPS),
)
ENEMY_TRAP_MASKS = (TRAP_MASKS[RED], TRAP_MASKS[BLUE])


def _can_enter(side: int, kind: int, square: int) -> bool:
    if square == OWN_DEN[side]:
        return False
    return kind == RAT or not ((WATER_MASK >> square) & 1)


OWN_DEN_DISTANCE = tuple(
    tuple(
        abs(square // 7 - OWN_DEN[side] // 7) + abs(square % 7 - OWN_DEN[side] % 7)
        for square in range(63)
    )
    for side in (BLUE, RED)
)

MANHATTAN_DEN_DISTANCE = tuple(
    tuple(
        abs(square // 7 - OPPONENT_DEN[side] // 7)
        + abs(square % 7 - OPPONENT_DEN[side] % 7)
        for square in range(63)
    )
    for side in (BLUE, RED)
)

ENTERABLE_NEIGHBORS = tuple(
    tuple(
        tuple(
            destination
            for destination in NEIGHBORS[square]
            if _can_enter(side, kind, destination)
        )
        for square in range(63)
    )
    for side in (BLUE, RED)
    for kind in range(PIECE_TYPE_COUNT + 1)
)


def evaluate_browser(position: BrowserCompactPosition) -> int:
    """Fast strategic evaluation from the side-to-move perspective."""

    scores = [0, 0]
    mobility = [0, 0]
    closest = [24, 24]
    attacks = [0, 0]
    defenders = [0, 0]
    rat_mask = position.piece_squares[RAT] | position.piece_squares[PIECE_TYPE_COUNT + RAT]
    elephants = [position.piece_counts[8], position.piece_counts[16]]

    for code in range(1, 2 * PIECE_TYPE_COUNT + 1):
        squares = position.piece_squares[code]
        if not squares:
            continue
        side = piece_side_fast(code)
        kind = piece_kind_fast(code)
        while squares:
            bit = squares & -squares
            square = bit.bit_length() - 1
            squares ^= bit

            value = PIECE_VALUES[kind]
            if ENEMY_TRAP_MASKS[side] & bit:
                value //= 3
            scores[side] += value

            distance = MANHATTAN_DEN_DISTANCE[side][square]
            closest[side] = min(closest[side], distance)
            scores[side] += (18 - distance) * 15
            if distance <= 3:
                scores[side] += (4 - distance) * (250 + (16 - position.total_piece_count) * 22)

            own_distance = OWN_DEN_DISTANCE[side][square]
            if own_distance <= 2:
                scores[side] += (3 - own_distance) * 65
            if ENEMY_TRAP_MASKS[side] & bit:
                scores[side] += 310
            elif TRAP_MASKS[side] & bit:
                scores[side] += 55

            if kind == RAT:
                if WATER_MASK & bit:
                    scores[side] += 130
                if elephants[side ^ 1]:
                    scores[side] += 75
            elif kind in (TIGER, LION) and 2 <= square // 7 <= 6:
                scores[side] += 55

            enterable = ENTERABLE_NEIGHBORS[side * (PIECE_TYPE_COUNT + 1) + kind][square]
            for destination in enterable:
                target = position.board[destination]
                if target == EMPTY:
                    mobility[side] += 1
                elif piece_side_fast(target) == side:
                    defenders[side] |= 1 << destination
                elif position._can_capture(square, destination):
                    attacks[side] |= 1 << destination
                    mobility[side] += 1

            if kind in (TIGER, LION):
                open_jumps = 0
                for jump in RIVER_JUMPS[square]:
                    if kind == TIGER and not jump.tiger_allowed:
                        continue
                    if jump.path_mask & rat_mask:
                        continue
                    open_jumps += 1
                    destination = jump.destination
                    target = position.board[destination]
                    if target == EMPTY:
                        mobility[side] += 1
                    elif piece_side_fast(target) == side:
                        defenders[side] |= 1 << destination
                    elif position._can_capture(square, destination):
                        attacks[side] |= 1 << destination
                        mobility[side] += 1
                # Preserve the proven positional balance while distinguishing a
                # genuinely open jump lane from one blocked by a Rat.
                scores[side] += open_jumps * 8

    phase = max(0, 16 - position.total_piece_count)
    for side in (BLUE, RED):
        scores[side] += mobility[side] * (7 + phase // 3)
        scores[side] += (14 - closest[side]) * (18 + phase * 4)

        threatened = attacks[side]
        while threatened:
            bit = threatened & -threatened
            threatened ^= bit
            square = bit.bit_length() - 1
            victim = position.board[square]
            victim_value = PIECE_VALUES[piece_kind_fast(victim)]
            scores[side] += victim_value // (12 if defenders[side ^ 1] & bit else 4)

        enemy_close = closest[side ^ 1]
        if enemy_close <= 3:
            scores[side] -= (4 - enemy_close) * (300 + phase * 25)

    absolute = scores[BLUE] - scores[RED]
    absolute += 12 if position.side_to_move == BLUE else -12
    return absolute if position.side_to_move == BLUE else -absolute


__all__ = [
    "FAST_TERMINAL_SCORE",
    "PIECE_VALUES",
    "TABLEBASE_SCORE",
    "evaluate_browser",
]
