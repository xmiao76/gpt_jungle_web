from __future__ import annotations

import json
import platform
from dataclasses import dataclass
from typing import Any, Callable

from jungle.ai import AlphaBetaAI, SearchConfig
from jungle.ai.tablebase import load_default_tablebase
from jungle.domain import (
    BOARD_SIZE,
    DEN_OWNER,
    WATER,
    GameState,
    Move,
    PieceType,
    ResultType,
    Side,
)
from jungle.engine import Game
from jungle.rules import effective_rank, evaluate_result


ENGINE_VERSION = "0.1.0"
DIFFICULTY_LIMITS = {"easy": 250, "medium": 750, "hard": 1_800}
DIFFICULTY_CONFIGS = {
    "easy": SearchConfig.baseline(),
    "medium": SearchConfig(
        label="medium",
        use_threat_score=True,
        use_quiescence=False,
        use_enhanced_ordering=True,
        threat_weight=1,
    ),
    "hard": SearchConfig.hard(),
}


class BrowserCommandError(ValueError):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


@dataclass(slots=True)
class RuntimeDetails:
    python_version: str
    engine_version: str
    tablebase_loaded: bool

    def to_dict(self) -> dict[str, Any]:
        return {
            "pythonVersion": self.python_version,
            "engineVersion": self.engine_version,
            "tablebaseLoaded": self.tablebase_loaded,
        }


def _require_dict(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise BrowserCommandError("INVALID_PAYLOAD", f"{label} must be an object.")
    return value


def _require_index(value: Any, label: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value < BOARD_SIZE:
        raise BrowserCommandError("INVALID_INDEX", f"{label} must be an integer from 0 to 62.")
    return value


def _difficulty(value: Any) -> str:
    if value not in DIFFICULTY_LIMITS:
        raise BrowserCommandError("INVALID_DIFFICULTY", "Difficulty must be easy, medium, or hard.")
    return str(value)


def _side(value: Any) -> Side:
    try:
        return Side(value)
    except (TypeError, ValueError) as exc:
        raise BrowserCommandError("INVALID_SIDE", "Side must be blue or red.") from exc


def _move_summary(move: Move, number: int | None = None) -> dict[str, Any]:
    value = move.to_dict()
    value.update(
        {
            "number": number,
            "pieceName": move.piece.kind.label,
            "sideName": move.piece.side.value.title(),
            "capturedName": None if move.captured is None else move.captured.kind.label,
            "notation": (
                f"{move.piece.side.value.title()} {move.piece.kind.label} "
                f"{move.origin}->{move.destination}"
                f"{' x ' + move.captured.kind.label if move.captured else ''}"
                f"{' (jump)' if move.is_jump else ''}"
            ),
        }
    )
    return value


def _validate_state_data(data: Any) -> GameState:
    state_data = _require_dict(data, "gameState")
    board_data = state_data.get("board")
    if not isinstance(board_data, list) or len(board_data) != BOARD_SIZE:
        raise BrowserCommandError("INVALID_SAVE", "A saved board must contain exactly 63 squares.")
    history_data = state_data.get("move_history", [])
    if not isinstance(history_data, list) or len(history_data) > 10_000:
        raise BrowserCommandError("INVALID_SAVE", "Move history is malformed or unreasonably large.")

    try:
        state = GameState.from_dict(state_data)
    except (KeyError, TypeError, ValueError) as exc:
        raise BrowserCommandError("INVALID_SAVE", f"Saved game fields are invalid: {exc}") from exc

    seen: set[tuple[Side, PieceType]] = set()
    for index, piece in enumerate(state.board):
        if piece is None:
            continue
        identity = (piece.side, piece.kind)
        if identity in seen:
            raise BrowserCommandError("INVALID_SAVE", "A side cannot contain duplicate animal types.")
        seen.add(identity)
        if index in WATER and piece.kind is not PieceType.RAT:
            raise BrowserCommandError("INVALID_SAVE", "Only rats may occupy water squares.")
        if index in DEN_OWNER and DEN_OWNER[index] is piece.side:
            raise BrowserCommandError("INVALID_SAVE", "A piece cannot occupy its own den.")

    expected_turn = Side.BLUE if len(state.move_history) % 2 == 0 else Side.RED
    if state.move_history and state.side_to_move is not expected_turn:
        raise BrowserCommandError("INVALID_SAVE", "Side to move is inconsistent with move history.")

    for ply, move in enumerate(state.move_history):
        _require_index(move.origin, f"move_history[{ply}].origin")
        _require_index(move.destination, f"move_history[{ply}].destination")
        expected_side = Side.BLUE if ply % 2 == 0 else Side.RED
        if move.piece.side is not expected_side:
            raise BrowserCommandError("INVALID_SAVE", "Move-history sides do not alternate from Blue.")
        if move.captured is not None and move.captured.side is move.piece.side:
            raise BrowserCommandError("INVALID_SAVE", "A move cannot capture a friendly piece.")

    if state.move_history:
        replay = Game()
        try:
            for move in state.move_history:
                replay.apply_coordinates(move.origin, move.destination)
        except ValueError as exc:
            raise BrowserCommandError("INVALID_SAVE", "Move history contains an illegal move.") from exc
        replay_data = replay.state.to_dict()
        for field in ("board", "side_to_move", "move_history", "winner", "result"):
            if replay_data[field] != state.to_dict()[field]:
                raise BrowserCommandError("INVALID_SAVE", f"Saved field '{field}' disagrees with move history.")

    probe = state.copy()
    probe.winner = None
    probe.result = ResultType.ONGOING
    probe.result_reason = ""
    expected_result = evaluate_result(probe)
    if state.result is ResultType.ONGOING:
        if expected_result.is_over:
            raise BrowserCommandError("INVALID_SAVE", "Saved game is marked ongoing but is already terminal.")
        if state.winner is not None:
            raise BrowserCommandError("INVALID_SAVE", "An ongoing game cannot identify a winner.")
    elif state.winner is not expected_result.winner or state.result is not expected_result.status:
        raise BrowserCommandError("INVALID_SAVE", "Saved terminal result does not match the board.")
    return state


class BrowserGameSession:
    def __init__(self) -> None:
        self.game = Game()
        self.difficulty = "medium"
        self.human_side = Side.BLUE
        self.revision = 0
        self.ai_stats: dict[str, Any] | None = None
        self.tablebase_loaded = load_default_tablebase() is not None
        self.runtime = RuntimeDetails(
            python_version=platform.python_version(),
            engine_version=ENGINE_VERSION,
            tablebase_loaded=self.tablebase_loaded,
        )
        self._handlers: dict[str, Callable[[dict[str, Any]], Any]] = {
            "newGame": self._new_game,
            "legalTargets": self._legal_targets,
            "applyMove": self._apply_move,
            "aiMove": self._ai_move,
            "undo": self._undo,
            "redo": self._redo,
            "importGame": self._import_game,
            "exportGame": self._export_game,
        }

    def handle(self, command_json: str) -> str:
        try:
            command = _require_dict(json.loads(command_json), "command")
            command_type = command.get("type")
            if command_type not in self._handlers:
                raise BrowserCommandError("UNKNOWN_COMMAND", f"Unsupported command: {command_type!r}.")
            payload = command.get("payload")
            if payload is None:
                payload = {}
            payload = _require_dict(payload, "payload")
            data = self._handlers[command_type](payload)
            result = {"ok": True, "revision": self.revision, "data": data}
        except BrowserCommandError as exc:
            result = {
                "ok": False,
                "revision": self.revision,
                "error": {"code": exc.code, "message": str(exc)},
            }
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
            result = {
                "ok": False,
                "revision": self.revision,
                "error": {"code": "INVALID_PAYLOAD", "message": str(exc)},
            }
        return json.dumps(result, separators=(",", ":"))

    def initial_snapshot_json(self) -> str:
        return json.dumps(self.snapshot(), separators=(",", ":"))

    def snapshot(self) -> dict[str, Any]:
        history = [
            _move_summary(move, number)
            for number, move in enumerate(self.game.state.move_history, start=1)
        ]
        return {
            "revision": self.revision,
            "gameState": self.game.state.to_dict(),
            "history": history,
            "lastMove": None if not history else history[-1],
            "canUndo": bool(self.game._undo_stack),
            "canRedo": bool(self.game._redo_stack),
            "difficulty": self.difficulty,
            "humanSide": self.human_side.value,
            "aiStats": self.ai_stats,
            "runtime": self.runtime.to_dict(),
        }

    def _new_game(self, payload: dict[str, Any]) -> dict[str, Any]:
        self.difficulty = _difficulty(payload.get("difficulty"))
        self.human_side = _side(payload.get("humanSide"))
        self.game.new_game()
        self.ai_stats = None
        self.revision += 1
        return self.snapshot()

    def _legal_targets(self, payload: dict[str, Any]) -> dict[str, Any]:
        origin = _require_index(payload.get("origin"), "origin")
        piece = self.game.state.board[origin]
        targets = []
        for move in self.game.list_moves():
            if move.origin == origin:
                targets.append(
                    {
                        "destination": move.destination,
                        "capture": move.captured is not None,
                        "captured": None if move.captured is None else move.captured.kind.label,
                        "jump": move.is_jump,
                        "note": move.note,
                    }
                )
        return {
            "origin": origin,
            "piece": None
            if piece is None
            else {"side": piece.side.value, "kind": piece.kind.name, "name": piece.kind.label},
            "effectiveRank": 0 if piece is None else effective_rank(self.game.state, origin),
            "targets": targets,
        }

    def _apply_move(self, payload: dict[str, Any]) -> dict[str, Any]:
        origin = _require_index(payload.get("origin"), "origin")
        destination = _require_index(payload.get("destination"), "destination")
        piece = self.game.state.board[origin]
        if piece is None:
            raise BrowserCommandError("ILLEGAL_MOVE", "No piece exists on the selected origin square.")
        if self.game.state.side_to_move is not self.human_side or piece.side is not self.human_side:
            raise BrowserCommandError("NOT_HUMAN_TURN", "Only the configured human side can make a manual move.")
        try:
            applied = self.game.apply_coordinates(origin, destination)
        except ValueError as exc:
            raise BrowserCommandError("ILLEGAL_MOVE", "That move is not legal in the current position.") from exc
        self.ai_stats = None
        self.revision += 1
        snapshot = self.snapshot()
        snapshot["appliedMove"] = _move_summary(applied.move, len(self.game.state.move_history))
        return snapshot

    def _ai_move(self, payload: dict[str, Any]) -> dict[str, Any]:
        difficulty = _difficulty(payload.get("difficulty", self.difficulty))
        self.difficulty = difficulty
        time_limit = payload.get("timeLimitMs", DIFFICULTY_LIMITS[difficulty])
        if not isinstance(time_limit, int) or isinstance(time_limit, bool):
            raise BrowserCommandError("INVALID_PAYLOAD", "timeLimitMs must be an integer.")
        time_limit = max(10, min(time_limit, 10_000))
        node_limit = payload.get("nodeLimit")
        if node_limit is not None:
            if not isinstance(node_limit, int) or isinstance(node_limit, bool):
                raise BrowserCommandError("INVALID_PAYLOAD", "nodeLimit must be an integer.")
            node_limit = max(1, min(node_limit, 1_000_000))

        result = AlphaBetaAI(
            time_limit,
            DIFFICULTY_CONFIGS[difficulty],
            node_limit=node_limit,
        ).choose_move(self.game.state.copy())
        if result.move is None:
            if self.game.state.winner is None:
                raise BrowserCommandError("AI_NO_MOVE", "The AI did not return a move for a non-terminal state.")
            return self.snapshot()

        applied = self.game.apply_move(result.move)
        self.ai_stats = {
            "difficulty": difficulty,
            "depth": result.depth,
            "nodes": result.nodes,
            "nodesPerSecond": result.nodes_per_second,
            "transpositionHits": result.tt_hits,
            "tablebaseHits": result.tablebase_hits,
            "elapsedMs": round(result.elapsed_ms, 3),
            "score": result.score,
            "principalVariation": [move.to_dict() for move in result.principal_variation],
        }
        self.revision += 1
        snapshot = self.snapshot()
        snapshot["appliedMove"] = _move_summary(applied.move, len(self.game.state.move_history))
        return snapshot

    def _undo(self, payload: dict[str, Any]) -> dict[str, Any]:
        plies = payload.get("plies", 1)
        if not isinstance(plies, int) or isinstance(plies, bool) or not 1 <= plies <= 16:
            raise BrowserCommandError("INVALID_PAYLOAD", "Undo plies must be an integer from 1 to 16.")
        applied = 0
        for _ in range(plies):
            if not self.game.undo():
                break
            applied += 1
        if applied:
            self.ai_stats = None
            self.revision += 1
        snapshot = self.snapshot()
        snapshot["appliedPlies"] = applied
        return snapshot

    def _redo(self, payload: dict[str, Any]) -> dict[str, Any]:
        plies = payload.get("plies", 1)
        if not isinstance(plies, int) or isinstance(plies, bool) or not 1 <= plies <= 16:
            raise BrowserCommandError("INVALID_PAYLOAD", "Redo plies must be an integer from 1 to 16.")
        applied = 0
        for _ in range(plies):
            if not self.game.redo():
                break
            applied += 1
        if applied:
            self.ai_stats = None
            self.revision += 1
        snapshot = self.snapshot()
        snapshot["appliedPlies"] = applied
        return snapshot

    def _import_game(self, payload: dict[str, Any]) -> dict[str, Any]:
        state = _validate_state_data(payload.get("gameState"))
        difficulty = _difficulty(payload.get("difficulty", self.difficulty))
        human_side = _side(payload.get("humanSide", self.human_side.value))
        self.game = Game(state)
        self.difficulty = difficulty
        self.human_side = human_side
        self.ai_stats = None
        self.revision += 1
        return self.snapshot()

    def _export_game(self, _payload: dict[str, Any]) -> dict[str, Any]:
        return {
            "gameState": self.game.state.to_dict(),
            "difficulty": self.difficulty,
            "humanSide": self.human_side.value,
            "engineVersion": ENGINE_VERSION,
        }


def create_session() -> BrowserGameSession:
    return BrowserGameSession()

