from __future__ import annotations

from jungle.ai.core import (
    BLUE,
    ELEPHANT,
    EMPTY,
    LION,
    NEIGHBORS,
    OWN_DEN,
    PIECE_TYPE_COUNT,
    RAT,
    RED,
    RIVER_JUMPS,
    TIGER,
    WATER_MASK,
    CompactPosition,
    PackedMove,
)
from jungle.domain import BLUE_TRAPS, RED_TRAPS


CAPTURE_SHIFT = 12
JUMP_MASK = 1 << 17
TRAP_OWNER = [-1] * 63
for _square in BLUE_TRAPS:
    TRAP_OWNER[_square] = BLUE
for _square in RED_TRAPS:
    TRAP_OWNER[_square] = RED


def piece_side_fast(piece: int) -> int:
    return BLUE if piece <= PIECE_TYPE_COUNT else RED


def piece_kind_fast(piece: int) -> int:
    return piece if piece <= PIECE_TYPE_COUNT else piece - PIECE_TYPE_COUNT


class BrowserCompactPosition(CompactPosition):
    """Allocation-light move generation for the browser Hard profile."""

    __slots__ = ()

    def generate_moves(self, side: int | None = None) -> list[PackedMove]:
        moving_side = self.side_to_move if side is None else side
        if moving_side not in (BLUE, RED):
            raise ValueError(f"Invalid side code: {moving_side}")
        if self.winner != -1:
            return []

        board = self.board
        piece_squares = self.piece_squares
        rat_mask = piece_squares[RAT] | piece_squares[PIECE_TYPE_COUNT + RAT]
        own_den = OWN_DEN[moving_side]
        code_start = 1 if moving_side == BLUE else PIECE_TYPE_COUNT + 1
        own_mask = 0
        for piece_code in range(code_start, code_start + PIECE_TYPE_COUNT):
            own_mask |= piece_squares[piece_code]

        moves: list[PackedMove] = []
        append = moves.append
        while own_mask:
            origin_bit = own_mask & -own_mask
            origin = origin_bit.bit_length() - 1
            own_mask ^= origin_bit
            piece = board[origin]
            kind = piece_kind_fast(piece)

            for destination in NEIGHBORS[origin]:
                if destination == own_den:
                    continue
                if (WATER_MASK >> destination) & 1 and kind != RAT:
                    continue
                target = board[destination]
                if target == EMPTY:
                    append(origin | (destination << 6))
                elif piece_side_fast(target) != moving_side and self._can_capture(origin, destination):
                    append(origin | (destination << 6) | (target << CAPTURE_SHIFT))

            if kind not in (TIGER, LION):
                continue
            for jump in RIVER_JUMPS[origin]:
                if kind == TIGER and not jump.tiger_allowed:
                    continue
                if jump.path_mask & rat_mask:
                    continue
                destination = jump.destination
                if destination == own_den:
                    continue
                target = board[destination]
                if target == EMPTY:
                    append(origin | (destination << 6) | JUMP_MASK)
                elif piece_side_fast(target) != moving_side and self._can_capture(origin, destination):
                    append(origin | (destination << 6) | (target << CAPTURE_SHIFT) | JUMP_MASK)
        return moves

    def _can_capture(self, attacker_square: int, defender_square: int) -> bool:
        attacker = self.board[attacker_square]
        defender = self.board[defender_square]
        if attacker == EMPTY or defender == EMPTY or piece_side_fast(attacker) == piece_side_fast(defender):
            return False

        attacker_kind = piece_kind_fast(attacker)
        defender_kind = piece_kind_fast(defender)
        attacker_in_water = bool((WATER_MASK >> attacker_square) & 1)
        defender_in_water = bool((WATER_MASK >> defender_square) & 1)
        if attacker_kind == RAT and defender_kind == ELEPHANT:
            return not attacker_in_water and not defender_in_water
        if attacker_kind == ELEPHANT and defender_kind == RAT:
            defender_side = piece_side_fast(defender)
            return TRAP_OWNER[defender_square] == (defender_side ^ 1)
        if attacker_in_water or defender_in_water:
            return attacker_in_water and defender_in_water and attacker_kind == defender_kind == RAT

        attacker_side = piece_side_fast(attacker)
        defender_side = piece_side_fast(defender)
        attacker_rank = 0 if TRAP_OWNER[attacker_square] == (attacker_side ^ 1) else attacker_kind
        defender_rank = 0 if TRAP_OWNER[defender_square] == (defender_side ^ 1) else defender_kind
        return attacker_rank >= defender_rank


__all__ = [
    "BrowserCompactPosition",
    "TRAP_OWNER",
    "piece_kind_fast",
    "piece_side_fast",
]
