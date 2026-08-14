from __future__ import annotations

from tools.web_ai_benchmark import tactical_cases


def test_browser_tactical_corpus_is_complete_and_distinct() -> None:
    cases = tactical_cases()

    assert len(cases) == 20
    assert len({case.name for case in cases}) == 20
    assert all(case.destinations for case in cases)
