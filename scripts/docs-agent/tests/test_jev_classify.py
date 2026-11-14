"""The analyze step asks Jev one Noul per doc category and falls back to the
chat-completions classifier when the decisions call fails."""

import client
import git_util
from steps import analyze

FILES = ["crates/mike/src/api/mod.rs", "scripts/deploy-proxy.sh"]


def _answers(**by_category: float) -> dict:
    return {name: {"type": "noul", "noul": p} for name, p in by_category.items()}


def _patch_git(monkeypatch) -> None:
    monkeypatch.setattr(git_util, "changed_files", lambda: FILES)
    monkeypatch.setattr(git_util, "diff_stat", lambda: "2 files changed")
    monkeypatch.setattr(git_util, "diff_full", lambda: "diff --git a b")


def test_each_category_is_true_at_or_above_one_half(monkeypatch):
    monkeypatch.setattr(
        client,
        "decide",
        lambda state, questions: _answers(architecture=0.9, schema=0.1, guides=0.5, ui_help=0.49),
    )
    found = analyze.classify_with_jev(FILES, "stat", "diff")
    assert found is not None
    assert (found["architecture"], found["schema"], found["guides"], found["ui_help"]) == (
        True,
        False,
        True,
        False,
    )
    assert "2 file(s) changed" in found["summary"]


def test_a_failed_call_gives_no_classification(monkeypatch):
    monkeypatch.setattr(client, "decide", lambda state, questions: None)
    assert analyze.classify_with_jev(FILES, "stat", "diff") is None


def test_a_missing_or_malformed_answer_gives_no_classification(monkeypatch):
    monkeypatch.setattr(
        client, "decide", lambda state, questions: _answers(architecture=0.9, schema=0.1, guides=0.2)
    )
    assert analyze.classify_with_jev(FILES, "stat", "diff") is None
    monkeypatch.setattr(
        client,
        "decide",
        lambda state, questions: _answers(architecture=0.9, schema=0.1, guides=0.2, ui_help=1.5),
    )
    assert analyze.classify_with_jev(FILES, "stat", "diff") is None


def test_run_uses_jev_and_never_calls_the_chat_model_when_it_answers(monkeypatch):
    _patch_git(monkeypatch)
    monkeypatch.setattr(
        client, "decide", lambda state, questions: _answers(architecture=0, schema=0, guides=1, ui_help=0)
    )

    def chat(*_args, **_kwargs):
        raise AssertionError("the chat model must not be called")

    monkeypatch.setattr(client, "complete", chat)
    found = analyze.run()
    assert found["guides"] is True
    assert found["architecture"] is False


def test_run_falls_back_to_the_chat_model_when_jev_fails(monkeypatch):
    _patch_git(monkeypatch)
    monkeypatch.setattr(client, "decide", lambda state, questions: None)
    monkeypatch.setattr(
        client,
        "complete",
        lambda system, user: (
            '{"summary": "s", "architecture": true, "schema": false, "guides": false, "ui_help": false}'
        ),
    )
    found = analyze.run()
    assert found["architecture"] is True
    assert found["summary"] == "s"


def test_decide_without_a_key_returns_none(monkeypatch):
    monkeypatch.undo()
    monkeypatch.delenv("OPENROUTER_API_KEY", raising=False)
    assert client.decide({}, {}) is None
