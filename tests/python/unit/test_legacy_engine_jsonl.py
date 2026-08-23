from __future__ import annotations

from io import StringIO
import json

from tools.legacy_engine_jsonl import LegacyEngineProtocol, ProtocolError, serve


def test_reference_protocol_returns_legal_fixed_node_move() -> None:
    protocol = LegacyEngineProtocol()
    _, hello = protocol.execute({"type": "hello", "id": 1})
    _, started = protocol.execute({"type": "newGame", "id": 2, "seed": 17})
    _, result = protocol.execute(
        {
            "type": "chooseMove",
            "id": 3,
            "limits": {"kind": "fixedNodes", "nodes": 200},
        }
    )

    assert hello["engineName"] == "web-hard-v2-reference"
    assert started["seed"] == 17
    assert result["move"] is not None
    _, applied = protocol.execute(
        {
            "type": "applyMove",
            "id": 4,
            "move": result["move"],
        }
    )
    assert len(applied["state"]["move_history"]) == 1


def test_reference_protocol_rejects_negative_coordinates() -> None:
    protocol = LegacyEngineProtocol()
    protocol.execute({"type": "newGame", "id": 1, "seed": 0})
    try:
        protocol.execute(
            {
                "type": "applyMove",
                "id": 2,
                "move": {"origin": -1, "destination": 6},
            }
        )
    except ProtocolError as exc:
        assert exc.code == "ILLEGAL_MOVE"
    else:
        raise AssertionError("negative coordinates must be rejected")


def test_jsonl_server_matches_the_language_neutral_arena_envelope() -> None:
    requests = StringIO(
        "\n".join(
            (
                json.dumps({"id": 7, "type": "hello"}),
                json.dumps({"id": 8, "type": "missing"}),
                json.dumps({"id": 9, "type": "close"}),
            )
        )
    )
    output = StringIO()

    assert serve(requests, output) == 0
    replies = [json.loads(line) for line in output.getvalue().splitlines()]
    assert replies[0]["id"] == 7 and replies[0]["ok"] and replies[0]["type"] == "hello"
    assert replies[1]["id"] == 8 and replies[1]["error"]["code"] == "INVALID_COMMAND"
    assert replies[2] == {"id": 9, "type": "close", "ok": True, "data": {}}
