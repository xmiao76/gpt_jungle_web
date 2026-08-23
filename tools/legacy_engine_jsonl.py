"""Strict JSONL adapter for the frozen ``web-hard-v2`` champion.

Production never imports this module.  It implements the same language-neutral
protocol as ``jungle-cli serve`` so the independent Python champion can play in
paired native arenas under identical node or wall-clock limits.
"""

from __future__ import annotations

import json
from pathlib import Path
import sys
from typing import Any, NoReturn, TextIO


ROOT = Path(__file__).resolve().parents[1]
PYTHON_ROOT = ROOT / "python"
if str(PYTHON_ROOT) not in sys.path:
    sys.path.insert(0, str(PYTHON_ROOT))

from browser_bridge import _validate_state_data
from jungle.ai.tablebase import RULES_HASH
from jungle.domain import ResultType
from jungle.engine import Game
from web_ai import HARD_AI_VERSION, BrowserHardAI


PROTOCOL_NAME = "jungle-jsonl"
PROTOCOL_VERSION = 1
RULESET_ID = "gpt-jungle-custom-v1"
RULES_SEMANTIC_VERSION = 2


class ProtocolError(ValueError):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


def _fail(code: str, message: str) -> NoReturn:
    raise ProtocolError(code, message)


def _require_id(request: dict[str, Any]) -> int:
    value = request.get("id")
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        _fail("INVALID_COMMAND", "id must be a nonnegative integer")
    return value


def _coordinates(move: Any) -> dict[str, int]:
    return {"origin": int(move.origin), "destination": int(move.destination)}


def _diagnostics(result: Any) -> dict[str, int]:
    return {
        "qnodes": 0,
        "evaluations": 0,
        "ttProbes": 0,
        "ttHits": int(result.tt_hits),
        "ttCutoffs": 0,
        "ttStores": 0,
        "ttCollisions": 0,
        "hashfullPerMille": 0,
        "tablebaseHits": int(result.tablebase_hits),
        "cycleHits": 0,
        "betaCutoffs": 0,
        "aspirationResearches": 0,
        "pvsResearches": 0,
        "lmrResearches": 0,
        "futilityPrunes": 0,
        "razorPrunes": 0,
        "deltaPrunes": 0,
    }


class LegacyEngineProtocol:
    def __init__(self) -> None:
        self.game: Game | None = None
        self.ai = BrowserHardAI()

    def execute(self, request: dict[str, Any]) -> tuple[str, dict[str, Any]]:
        request_id = _require_id(request)
        command = request.get("type")
        if command == "hello":
            return "hello", {
                "protocol": PROTOCOL_NAME,
                "protocolVersion": PROTOCOL_VERSION,
                "engineName": "web-hard-v2-reference",
                "engineVersion": "0.2.0-reference",
                "rulesetId": RULESET_ID,
                "rulesSemanticVersion": RULES_SEMANTIC_VERSION,
                "capabilities": {
                    "fixedNodes": True,
                    "exactTime": True,
                    "persistentGameState": True,
                    "structuredDiagnostics": True,
                },
            }
        if command == "newGame":
            seed = request.get("seed", 0)
            if not isinstance(seed, int) or isinstance(seed, bool) or seed < 0:
                _fail("INVALID_COMMAND", "seed must be a nonnegative integer")
            self.game = Game()
            self.ai.reset()
            return "newGame", {"state": self.game.state.to_dict(), "seed": seed}
        if command == "setPosition":
            raw_state = request.get("state")
            if not isinstance(raw_state, dict):
                _fail("INVALID_POSITION", "setPosition requires a state object")
            try:
                self.game = Game(_validate_state_data(raw_state))
            except (KeyError, TypeError, ValueError) as exc:
                raise ProtocolError("INVALID_POSITION", str(exc)) from exc
            self.ai.reset()
            return "setPosition", {"state": self.game.state.to_dict()}
        if command == "chooseMove":
            game = self._game()
            if game.state.result is not ResultType.ONGOING:
                _fail("GAME_OVER", "game is already over")
            limits = request.get("limits")
            if not isinstance(limits, dict):
                _fail("INVALID_LIMIT", "chooseMove requires a limits object")
            kind = limits.get("kind")
            max_depth = limits.get("maxDepth")
            if max_depth is not None and (
                not isinstance(max_depth, int)
                or isinstance(max_depth, bool)
                or not 1 <= max_depth <= 128
            ):
                _fail("INVALID_LIMIT", "maxDepth must be in 1..=128")
            if kind == "fixedNodes":
                node_limit = limits.get("nodes")
                if (
                    not isinstance(node_limit, int)
                    or isinstance(node_limit, bool)
                    or node_limit <= 0
                ):
                    _fail("INVALID_LIMIT", "fixedNodes requires nodes greater than zero")
                time_ms = 60_000
                termination = "node_limit"
            elif kind == "exactTime":
                time_ms = limits.get("timeMs")
                if (
                    not isinstance(time_ms, int)
                    or isinstance(time_ms, bool)
                    or time_ms <= 0
                ):
                    _fail("INVALID_LIMIT", "exactTime requires timeMs greater than zero")
                node_limit = None
                termination = "hard_deadline"
            else:
                _fail("INVALID_LIMIT", f"unsupported limit kind: {kind!r}")

            result = self.ai.choose_move(game.state.copy(), time_ms, node_limit)
            if result.move is None:
                _fail("SEARCH_FAILED", "reference search returned no legal move")
            selected = _coordinates(result.move)
            principal_variation = [_coordinates(move) for move in result.principal_variation]
            if not principal_variation or principal_variation[0] != selected:
                # The frozen engine occasionally leaves a prior completed PV
                # after a deadline fallback. Its selected move is authoritative.
                principal_variation = [selected]
            return "chooseMove", {
                "move": selected,
                "search": {
                    "score": int(result.score),
                    "depth": int(result.depth),
                    "selectiveDepth": int(result.depth),
                    "nodes": int(result.nodes),
                    "elapsedMs": float(result.elapsed_ms),
                    "nodesPerSecond": int(result.nodes_per_second),
                    "transpositionHits": int(result.tt_hits),
                    "tablebaseHits": int(result.tablebase_hits),
                    "principalVariation": principal_variation,
                    "iterations": [],
                    "termination": termination,
                    "diagnostics": _diagnostics(result),
                },
            }
        if command == "applyMove":
            game = self._game()
            selected_move = request.get("move")
            if not isinstance(selected_move, dict):
                _fail("ILLEGAL_MOVE", "applyMove requires a move object")
            origin = selected_move.get("origin")
            destination = selected_move.get("destination")
            if (
                not isinstance(origin, int)
                or isinstance(origin, bool)
                or not 0 <= origin < 63
                or not isinstance(destination, int)
                or isinstance(destination, bool)
                or not 0 <= destination < 63
            ):
                _fail("ILLEGAL_MOVE", "move coordinates must be integers from 0 to 62")
            try:
                applied = game.apply_coordinates(origin, destination)
            except ValueError as exc:
                raise ProtocolError("ILLEGAL_MOVE", str(exc)) from exc
            return "applyMove", {
                "appliedMove": _coordinates(applied.move),
                "state": game.state.to_dict(),
            }
        if command == "close":
            return "close", {}
        _fail("INVALID_COMMAND", f"unknown command: {command!r}")

    def _game(self) -> Game:
        if self.game is None:
            _fail("NOT_INITIALIZED", "newGame or setPosition is required")
        return self.game


def serve(input_stream: TextIO = sys.stdin, output_stream: TextIO = sys.stdout) -> int:
    protocol = LegacyEngineProtocol()
    for raw_line in input_stream:
        line = raw_line.strip()
        if not line:
            continue
        request_id: int | None = None
        close = False
        try:
            value = json.loads(line)
            if not isinstance(value, dict):
                _fail("INVALID_JSON", "each request must be a JSON object")
            raw_id = value.get("id")
            request_id = raw_id if isinstance(raw_id, int) and not isinstance(raw_id, bool) else None
            response_type, data = protocol.execute(value)
            close = response_type == "close"
            response = {
                "id": request_id,
                "type": response_type,
                "ok": True,
                "data": data,
            }
        except json.JSONDecodeError as exc:
            response = {
                "id": None,
                "type": "error",
                "ok": False,
                "error": {"code": "INVALID_JSON", "message": str(exc)},
            }
        except ProtocolError as exc:
            response = {
                "id": request_id,
                "type": "error",
                "ok": False,
                "error": {"code": exc.code, "message": str(exc)},
            }
        except (KeyError, TypeError, ValueError) as exc:
            response = {
                "id": request_id,
                "type": "error",
                "ok": False,
                "error": {"code": "INVALID_COMMAND", "message": str(exc)},
            }
        output_stream.write(json.dumps(response, separators=(",", ":")) + "\n")
        output_stream.flush()
        if close and response["ok"]:
            break
    return 0


def main() -> int:
    return serve()


if __name__ == "__main__":
    raise SystemExit(main())
