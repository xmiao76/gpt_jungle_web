from __future__ import annotations

import math
import time

from jungle.ai.core import EMPTY, NO_SIDE, PackedMove, move_captured, move_destination
from jungle.ai.fast_search import CompactSearch, CompactSearchAborted
from jungle.ai.search import SearchConfig, SearchResult
from jungle.ai.transposition import Bound
from jungle.domain import BLUE_TRAPS, RED_TRAPS, GameState

from .evaluation import FAST_TERMINAL_SCORE, PIECE_VALUES, evaluate_browser
from .position import BrowserCompactPosition, piece_kind_fast


DESTINATION_BONUS = tuple(
    tuple(
        (18 - abs(square // 7 - target // 7) - abs(square % 7 - target % 7)) * 100
        for square in range(63)
    )
    for target in (3, 59)
)
TARGET_DEN = (3, 59)
TARGET_TRAPS = (frozenset(RED_TRAPS), frozenset(BLUE_TRAPS))


class BrowserCompactSearch(CompactSearch):
    """Persistent browser Hard search using the upstream search contract."""

    def _check_limits(self) -> None:
        if self.node_limit is not None and self.nodes >= self.node_limit:
            raise CompactSearchAborted
        # perf_counter is comparatively expensive in Pyodide. Checking every
        # 32 nodes bounds ordinary deadline overshoot to a few milliseconds
        # while retaining exact deterministic node-limit behavior.
        if not (self.nodes & 31) and time.perf_counter() >= self.deadline:
            raise CompactSearchAborted

    def choose_move(self, state: GameState) -> SearchResult:
        started = time.perf_counter()
        self.deadline = started + self.time_limit_ms / 1_000.0
        self.nodes = 0
        self.killers.clear()
        self.history.clear()
        self.table.new_search()
        self.tablebase_hits = 0
        self.root_history = tuple(state.move_history[-8:])
        position = BrowserCompactPosition.from_game_state(state)
        self.path_counts = self._recent_hash_counts(state)
        self.path_counts[position.zobrist_hash] = max(1, self.path_counts[position.zobrist_hash])

        moves = position.generate_moves()
        winner = position.terminal_winner(moves)
        if not moves:
            return SearchResult(
                None,
                -FAST_TERMINAL_SCORE,
                0,
                0,
                (time.perf_counter() - started) * 1_000,
            )
        if winner is not None:
            score = FAST_TERMINAL_SCORE if winner == position.side_to_move else -FAST_TERMINAL_SCORE
            return SearchResult(None, score, 0, 0, (time.perf_counter() - started) * 1_000)

        winning = next(
            (move for move in moves if move_destination(move) == self._enemy_den(position.side_to_move)),
            None,
        )
        if winning is not None:
            public = position.to_public_move(winning)
            return SearchResult(
                public,
                FAST_TERMINAL_SCORE - 1,
                1,
                0,
                (time.perf_counter() - started) * 1_000,
                (public,),
                0,
                0,
                0,
            )

        tb_move, tb_score = self._root_tablebase_move(position, moves)
        if tb_move is not None:
            public = position.to_public_move(tb_move)
            return SearchResult(
                public,
                tb_score,
                0,
                0,
                (time.perf_counter() - started) * 1_000,
                (public,),
                0,
                self.table.hits,
                self.tablebase_hits,
            )

        root_hint = 0
        previous = self.table.probe(position.zobrist_hash)
        if previous is not None and previous.move in moves:
            root_hint = previous.move
        ordered_fallback = self._order_moves(position, moves, root_hint, 0)
        best_move = ordered_fallback[0]
        best_score = evaluate_browser(position)
        completed_depth = 0
        previous_score = 0

        for depth in range(1, self.config.max_depth + 1):
            try:
                self._check_limits()
                if self.config.use_aspiration_windows and completed_depth:
                    window = max(50, self.config.aspiration_window)
                    alpha = previous_score - window
                    beta = previous_score + window
                    score, move = self._search_root(position, moves, depth, best_move, alpha, beta)
                    if score <= alpha or score >= beta:
                        score, move = self._search_root(
                            position, moves, depth, best_move, -math.inf, math.inf
                        )
                else:
                    score, move = self._search_root(
                        position, moves, depth, best_move, -math.inf, math.inf
                    )
            except CompactSearchAborted:
                break
            if move:
                best_move = move
                best_score = score
                previous_score = score
                completed_depth = depth

        elapsed = (time.perf_counter() - started) * 1_000
        public_move = position.to_public_move(best_move)
        public_pv = self._extract_public_pv(position, completed_depth) or (public_move,)
        nps = int(self.nodes * 1_000 / elapsed) if elapsed > 0 else 0
        return SearchResult(
            public_move,
            int(best_score),
            completed_depth,
            self.nodes,
            elapsed,
            public_pv,
            nps,
            self.table.hits,
            self.tablebase_hits,
        )

    def _negamax(
        self,
        position: BrowserCompactPosition,
        depth: int,
        alpha: float,
        beta: float,
        ply: int,
        extensions_used: int,
    ) -> int:
        self._visit_node()
        if position.winner != NO_SIDE:
            return self._terminal_score(position, ply)

        tablebase_score = self._tablebase_score(position)
        if tablebase_score is not None:
            return tablebase_score

        alpha_original = alpha
        beta_original = beta
        table_index = position.zobrist_hash & self.table.mask
        is_current_entry = self.table.generations[table_index] == self.table.generation
        entry = self.table.probe(position.zobrist_hash)
        preferred = 0
        if entry is not None:
            # Old-generation scores can depend on a different recent repetition
            # history.  Their move remains a safe ordering hint, but only bounds
            # produced in this search may cut off the current tree.
            preferred = entry.move
            if is_current_entry and entry.depth >= depth:
                score = self._score_from_tt(entry.score, ply)
                if entry.bound is Bound.EXACT:
                    return score
                if entry.bound is Bound.LOWER:
                    alpha = max(alpha, score)
                else:
                    beta = min(beta, score)
                if alpha >= beta:
                    return score

        moves = position.generate_moves()
        if not moves:
            return -FAST_TERMINAL_SCORE + ply
        if depth <= 0:
            score = (
                self._quiescence(position, alpha, beta, ply, 0)
                if self.config.use_quiescence
                else evaluate_browser(position)
            )
            bound = Bound.EXACT
            if score <= alpha_original:
                bound = Bound.UPPER
            elif score >= beta_original:
                bound = Bound.LOWER
            self.table.store(position.zobrist_hash, 0, self._score_to_tt(score, ply), bound, 0)
            return score

        best_score = -math.inf
        best_move = 0
        ordered = self._order_moves(position, moves, preferred, ply)
        for index, move in enumerate(ordered):
            extension = 0
            if extensions_used == 0 and self._is_den_pressure_move(position.side_to_move, move):
                extension = 1
            child_depth = depth - 1 + extension
            undo = position.make_move(move)
            try:
                if self._should_reduce(move, index, depth, ply):
                    score = self._score_child(
                        position,
                        max(0, child_depth - 1),
                        alpha,
                        alpha + 1,
                        ply + 1,
                        True,
                        extensions_used + extension,
                    )
                    if score > alpha:
                        score = self._score_child(
                            position,
                            child_depth,
                            alpha,
                            beta,
                            ply + 1,
                            index > 0,
                            extensions_used + extension,
                        )
                else:
                    score = self._score_child(
                        position,
                        child_depth,
                        alpha,
                        beta,
                        ply + 1,
                        index > 0,
                        extensions_used + extension,
                    )
            finally:
                position.unmake_move(move, undo)
            if score > best_score:
                best_score = score
                best_move = move
            alpha = max(alpha, score)
            if alpha >= beta:
                self._record_cutoff(position.side_to_move, move, depth, ply)
                break

        bound = Bound.EXACT
        if best_score <= alpha_original:
            bound = Bound.UPPER
        elif best_score >= beta_original:
            bound = Bound.LOWER
        self.table.store(
            position.zobrist_hash,
            depth,
            self._score_to_tt(int(best_score), ply),
            bound,
            best_move,
        )
        return int(best_score)

    def _quiescence(
        self,
        position: BrowserCompactPosition,
        alpha: float,
        beta: float,
        ply: int,
        qply: int,
    ) -> int:
        self._visit_node()
        if position.winner != NO_SIDE:
            return self._terminal_score(position, ply)
        tablebase_score = self._tablebase_score(position)
        if tablebase_score is not None:
            return tablebase_score

        moves = position.generate_moves()
        if not moves:
            return -FAST_TERMINAL_SCORE + ply
        under_threat = self._has_immediate_den_threat(position)
        stand_pat = evaluate_browser(position)
        if not under_threat:
            if stand_pat >= beta:
                return int(stand_pat)
            alpha = max(alpha, stand_pat)
        max_qply = max(1, min(6, self.config.quiescence_max_depth or 6))
        if qply >= max_qply:
            return int(stand_pat if under_threat else alpha)

        if under_threat:
            candidates = [move for move in moves if self._defends_den(position, move)]
            if not candidates:
                return -FAST_TERMINAL_SCORE + ply
        else:
            target_den = TARGET_DEN[position.side_to_move]
            candidates = [
                move
                for move in moves
                if move_captured(move) != EMPTY
                or move_destination(move) == target_den
            ]
        if not candidates:
            return int(alpha)

        ordered = self._order_moves(position, candidates, 0, ply)
        if not under_threat and self.config.quiescence_candidate_limit:
            ordered = ordered[: self.config.quiescence_candidate_limit]
        for move in ordered:
            self._check_limits()
            undo = position.make_move(move)
            key = position.zobrist_hash
            try:
                if self.path_counts[key] > 0:
                    score = 0
                else:
                    self.path_counts[key] += 1
                    try:
                        score = -self._quiescence(position, -beta, -alpha, ply + 1, qply + 1)
                    finally:
                        self.path_counts[key] -= 1
                        if self.path_counts[key] == 0:
                            del self.path_counts[key]
            finally:
                position.unmake_move(move, undo)
            if score >= beta:
                return int(score)
            alpha = max(alpha, score)
        return int(alpha)

    def _order_moves(
        self,
        position: BrowserCompactPosition,
        moves: list[PackedMove],
        preferred: PackedMove,
        ply: int,
    ) -> list[PackedMove]:
        side = position.side_to_move
        target_den = TARGET_DEN[side]
        target_traps = TARGET_TRAPS[side]
        board = position.board
        killers = self.killers.get(ply, ())
        history = self.history
        destination_bonus = DESTINATION_BONUS[side]
        ranked: list[tuple[int, int, PackedMove]] = []
        append = ranked.append

        for ordinal, move in enumerate(moves):
            destination = (move >> 6) & 0x3F
            captured = (move >> 12) & 0x1F
            value = destination_bonus[destination]
            if preferred and move == preferred:
                value += 2_000_000
            if destination == target_den:
                value += 1_500_000
            if captured:
                attacker = board[move & 0x3F]
                value += 200_000 + PIECE_VALUES[piece_kind_fast(captured)] * 32
                value -= PIECE_VALUES[piece_kind_fast(attacker)]
            if destination in target_traps:
                value += 18_000
            if move & (1 << 17):
                value += 2_500
            if move in killers:
                value += 35_000
            value += history.get((side, move), 0)
            if ply == 0 and self._is_root_reversal(position, move):
                value -= 4_000
            append((value, -ordinal, move))

        ranked.sort(reverse=True)
        return [move for _score, _ordinal, move in ranked]

    def _terminal_score(self, position: BrowserCompactPosition, ply: int) -> int:
        if position.winner == NO_SIDE:
            return evaluate_browser(position)
        if position.winner == position.side_to_move:
            return FAST_TERMINAL_SCORE - ply
        return -FAST_TERMINAL_SCORE + ply


class BrowserHardAI:
    """Session-scoped owner for the browser-only Hard search."""

    def __init__(self, config: SearchConfig | None = None) -> None:
        self.config = SearchConfig.hard() if config is None else config
        self._search = BrowserCompactSearch(1_800, self.config)

    def reset(self) -> None:
        self._search = BrowserCompactSearch(1_800, self.config)

    def choose_move(
        self,
        state: GameState,
        time_limit_ms: int = 1_800,
        node_limit: int | None = None,
    ) -> SearchResult:
        self._search.time_limit_ms = time_limit_ms
        self._search.node_limit = node_limit
        return self._search.choose_move(state)


__all__ = ["BrowserCompactSearch", "BrowserHardAI"]
