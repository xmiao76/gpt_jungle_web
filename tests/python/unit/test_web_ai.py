from __future__ import annotations

import random

from jungle.ai.core import EMPTY, compact_piece, compact_side
from jungle.ai.tablebase import load_default_tablebase
from jungle.domain import GameState, Piece, PieceType, Position, Side, initial_state
from jungle.engine import Game
from jungle.rules import legal_moves
from web_ai import HARD_AI_VERSION, BrowserHardAI
from web_ai.evaluation import evaluate_browser
from web_ai.position import BrowserCompactPosition


def snapshot(position: BrowserCompactPosition) -> tuple[object, ...]:
    return (
        position.board.copy(),
        position.piece_squares.copy(),
        position.piece_counts.copy(),
        position.side_counts.copy(),
        position.side_to_move,
        position.total_piece_count,
        position.winner,
        position.zobrist_hash,
    )


def assert_matches_state(position: BrowserCompactPosition, state: GameState) -> None:
    assert position.board == [EMPTY if piece is None else compact_piece(piece) for piece in state.board]
    assert position.side_to_move == compact_side(state.side_to_move)
    assert position.zobrist_hash == position.recompute_zobrist()
    position.assert_consistent()


def make_state(pieces: dict[int, Piece], side: Side = Side.BLUE) -> GameState:
    board = [None] * 63
    for square, piece in pieces.items():
        board[square] = piece
    return GameState(board, side)


def test_browser_hard_version_is_explicit() -> None:
    assert HARD_AI_VERSION == "web-hard-v2"


def test_browser_move_generation_matches_authoritative_rules_and_restores() -> None:
    rng = random.Random(0xB00A5E)
    checked = 0
    for _ in range(6):
        game = Game()
        position = BrowserCompactPosition.from_game_state(game.state)
        for _ply in range(35):
            expected = legal_moves(game.state)
            packed = position.generate_moves()
            assert [position.to_public_move(move) for move in packed] == expected
            assert_matches_state(position, game.state)
            checked += 1
            if not expected:
                break

            selected = rng.randrange(len(expected))
            before = snapshot(position)
            undo = position.make_move(packed[selected])
            position.unmake_move(packed[selected], undo)
            assert snapshot(position) == before

            position.make_move(packed[selected])
            game.apply_move(expected[selected])
            assert_matches_state(position, game.state)
            if game.state.winner is not None:
                break

    assert checked >= 100


def test_browser_evaluation_preserves_turn_perspective_and_rewards_material() -> None:
    pieces = {
        Position(6, 0).index: Piece(Side.BLUE, PieceType.ELEPHANT),
        Position(2, 6).index: Piece(Side.RED, PieceType.CAT),
    }
    blue = BrowserCompactPosition.from_game_state(make_state(pieces, Side.BLUE))
    red = BrowserCompactPosition.from_game_state(make_state(pieces, Side.RED))

    assert evaluate_browser(blue) > 0
    assert evaluate_browser(blue) - 12 == -(evaluate_browser(red) - 12)


def test_browser_hard_uses_exact_tablebase_and_returns_legal_pv() -> None:
    state = make_state(
        {
            Position(4, 0).index: Piece(Side.BLUE, PieceType.CAT),
            Position(4, 6).index: Piece(Side.RED, PieceType.DOG),
        }
    )
    tablebase = load_default_tablebase()
    assert tablebase is not None

    result = BrowserHardAI().choose_move(state, time_limit_ms=300)

    assert result.move == tablebase.choose_move(state)
    assert result.tablebase_hits > 0
    assert result.depth == 0


def test_browser_hard_honors_exact_node_limit_and_keeps_completed_iteration() -> None:
    state = initial_state()
    result = BrowserHardAI().choose_move(state, time_limit_ms=10_000, node_limit=2_000)

    assert result.move in legal_moves(state)
    assert result.nodes <= 2_000
    assert result.depth > 0
    assert result.principal_variation


def test_persistent_table_uses_previous_search_only_as_legal_move_hint() -> None:
    ai = BrowserHardAI()
    game = Game()
    first = ai.choose_move(game.state.copy(), time_limit_ms=10_000, node_limit=600)
    assert first.move in game.list_moves()
    game.apply_move(first.move)

    second = ai.choose_move(game.state.copy(), time_limit_ms=10_000, node_limit=600)

    assert second.move in game.list_moves()
    assert second.tt_hits > 0
