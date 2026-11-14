"""client.decide asks through mike's decisions route so every Jev call is
recorded, and gives None whenever mike did not decide."""

import httpx

import client

_decide = client.decide


class _Reply:
    def __init__(self, status: int, body):
        self.status_code = status
        self.is_success = 200 <= status < 300
        self._body = body
        self.text = str(body)

    def json(self):
        return self._body


def _post_answering(seen: list, status: int, body):
    def post(url, json, timeout):
        seen.append((url, json))
        return _Reply(status, body)

    return post


def test_a_jev_answer_from_mike_is_returned_and_names_the_point(monkeypatch):
    seen: list = []
    answers = {"schema": {"type": "noul", "noul": 0.8}}
    monkeypatch.setattr(
        httpx, "post", _post_answering(seen, 200, {"decided_by": "jev", "record_id": 7, "answers": answers})
    )
    assert _decide({"changed_files": []}, {"schema": {}}) == answers
    url, sent = seen[0]
    assert url.endswith("/api/decisions/ask")
    assert sent["point"] == "docs_agent"
    assert sent["question_set"] == "docs_agent_v1"
    assert sent["state"] == {"changed_files": []}


def test_the_layer_off_gives_no_answers(monkeypatch):
    monkeypatch.setattr(httpx, "post", _post_answering([], 200, {"decided_by": "off", "answers": None}))
    assert _decide({}, {}) is None


def test_a_refused_or_failed_call_gives_no_answers(monkeypatch):
    monkeypatch.setattr(httpx, "post", _post_answering([], 502, {"error": "jev failed"}))
    assert _decide({}, {}) is None


def test_mike_not_running_gives_no_answers(monkeypatch):
    def post(url, json, timeout):
        raise httpx.ConnectError("refused")

    monkeypatch.setattr(httpx, "post", post)
    assert _decide({}, {}) is None
