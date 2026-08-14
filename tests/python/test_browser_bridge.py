from __future__ import annotations

import json

from browser_bridge import BrowserGameSession


def command(session: BrowserGameSession, command_type: str, payload: dict | None = None) -> dict:
    return json.loads(session.handle(json.dumps({"type": command_type, "payload": payload or {}})))


def test_session_snapshot_and_authoritative_targets() -> None:
    session = BrowserGameSession()
    snapshot = json.loads(session.initial_snapshot_json())
    assert len(snapshot["gameState"]["board"]) == 63
    assert sum(piece is not None for piece in snapshot["gameState"]["board"]) == 16
    assert snapshot["runtime"]["tablebaseLoaded"] is True
    assert snapshot["runtime"]["engineVersion"] == "0.2.0"
    assert snapshot["runtime"]["hardAiVersion"] == "web-hard-v2"

    reply = command(session, "legalTargets", {"origin": 42})
    assert reply["ok"] is True
    assert {target["destination"] for target in reply["data"]["targets"]} == {35, 43, 49}


def test_human_move_undo_and_exact_redo() -> None:
    session = BrowserGameSession()
    moved = command(session, "applyMove", {"origin": 42, "destination": 35})
    assert moved["ok"] is True
    assert moved["data"]["gameState"]["side_to_move"] == "red"

    undone = command(session, "undo", {"plies": 1})
    assert undone["data"]["gameState"]["board"][42]["kind"] == "ELEPHANT"
    assert undone["data"]["canRedo"] is True

    redone = command(session, "redo", {"plies": 1})
    assert redone["data"]["gameState"]["board"][35]["kind"] == "ELEPHANT"
    assert redone["data"]["history"][0]["destination"] == 35


def test_import_rejects_invalid_board_without_mutating_session() -> None:
    session = BrowserGameSession()
    before = session.snapshot()
    reply = command(
        session,
        "importGame",
        {"gameState": {"board": []}, "difficulty": "easy", "humanSide": "blue"},
    )
    assert reply["ok"] is False
    assert reply["error"]["code"] == "INVALID_SAVE"
    assert session.snapshot()["gameState"] == before["gameState"]


def test_ai_move_with_node_limit_is_legal_and_reports_stats() -> None:
    session = BrowserGameSession()
    reply = command(session, "aiMove", {"difficulty": "easy", "timeLimitMs": 10_000, "nodeLimit": 100})
    assert reply["ok"] is True
    assert len(reply["data"]["history"]) == 1
    assert reply["data"]["aiStats"]["nodes"] <= 100


def test_hard_ai_dispatches_to_browser_overlay() -> None:
    session = BrowserGameSession()
    reply = command(
        session,
        "aiMove",
        {"difficulty": "hard", "timeLimitMs": 10_000, "nodeLimit": 500},
    )

    assert reply["ok"] is True
    assert reply["data"]["runtime"]["hardAiVersion"] == "web-hard-v2"
    assert reply["data"]["aiStats"]["difficulty"] == "hard"
    assert reply["data"]["aiStats"]["nodes"] <= 500
