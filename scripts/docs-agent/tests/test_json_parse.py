"""Tolerant JSON parsing of model responses: git_util.parse_json_lenient, and
the analyze step's fail-safe fallback when a response can't be parsed into a
valid classification. Reproduces the run-28584000003 crash — json.loads choking
on "Extra data" when the model decorates its JSON reply — the same decoration
problem is_no_change already handles for NO_CHANGES sentinels."""

import json

import client
import git_util
from steps import analyze

EXPECTED = {
    "summary": "Adds WebSocket upgrade support to proxy-forward",
    "architecture": True,
    "schema": False,
    "guides": False,
    "ui_help": False,
}


def _payload() -> str:
    return json.dumps(EXPECTED, indent=2)


# ── parse_json_lenient ──────────────────────────────────────────────────────


def test_fenced_object_with_trailing_prose_parses():
    raw = f"```json\n{_payload()}\n```\n\nThis affects the request flow docs."
    assert git_util.parse_json_lenient(raw) == EXPECTED


def test_bare_object_parses():
    assert git_util.parse_json_lenient(_payload()) == EXPECTED


def test_extra_data_after_object_parses():
    # Exact repro of the run-28584000003 crash: valid JSON immediately
    # followed by an unfenced explanation — json.loads raises
    # "Extra data: line 8 column 1"; raw_decode must tolerate it.
    raw = _payload() + "\n\nThis change affects the request flow docs."
    assert git_util.parse_json_lenient(raw) == EXPECTED


def test_non_json_response_returns_none():
    raw = "I cannot classify this diff without more context."
    assert git_util.parse_json_lenient(raw) is None


def test_empty_response_returns_none():
    assert git_util.parse_json_lenient("") is None
    assert git_util.parse_json_lenient("   \n  ") is None


def test_non_object_json_returns_none():
    assert git_util.parse_json_lenient("[1, 2, 3]") is None
    assert git_util.parse_json_lenient('"just a string"') is None


# ── analyze.run(): fail-safe on unparseable/malformed responses ────────────


def test_analyze_survives_non_json_response(monkeypatch):
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: "I cannot classify this diff.",
    )

    result = analyze.run()

    assert result["architecture"] is False
    assert result["schema"] is False
    assert result["guides"] is False
    assert result["ui_help"] is False
    assert isinstance(result["summary"], str) and result["summary"]


def test_analyze_survives_extra_data_response(monkeypatch):
    # The literal failure mode from the bug report: valid JSON, then prose.
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: _payload()
        + "\n\nThis change affects the request flow docs.",
    )

    result = analyze.run()

    assert result == EXPECTED


def test_analyze_survives_malformed_shape(monkeypatch):
    # Valid JSON, but missing the keys __main__.py indexes into.
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user, model=None: '{"unexpected": "shape"}',
    )

    result = analyze.run()

    assert result["architecture"] is False
    assert result["schema"] is False
    assert result["guides"] is False
    assert result["ui_help"] is False


# ── analyze.run(): retries a decorated first reply instead of giving up ────


def test_analyze_retries_and_recovers_real_classification(monkeypatch):
    # A malformed first reply is a formatting slip, not "nothing to update" —
    # the retry must still deliver the real classification so a commit that
    # genuinely needs docs updated isn't silently skipped.
    responses = iter(
        [
            "Sure! Here's my analysis in plain English, not JSON.",
            _payload(),
        ]
    )
    calls: list[str] = []

    def fake_complete(system, user, model=None):
        calls.append(user)
        return next(responses)

    monkeypatch.setattr(client, "complete", fake_complete)

    result = analyze.run()

    assert result == EXPECTED
    assert len(calls) == 2
    # The retry must call out the exact bad output so the model can self-correct.
    assert "could not be parsed as JSON" in calls[1]


def test_analyze_gives_up_after_exactly_one_retry(monkeypatch):
    call_count = 0

    def fake_complete(system, user, model=None):
        nonlocal call_count
        call_count += 1
        return "still not JSON, no matter how many times you ask"

    monkeypatch.setattr(client, "complete", fake_complete)

    result = analyze.run()

    assert call_count == 2
    assert result["architecture"] is False
    assert result["schema"] is False
    assert result["guides"] is False
    assert result["ui_help"] is False
