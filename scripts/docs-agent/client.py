"""OpenRouter API client — thin wrapper around the chat completions endpoint."""

import os
import httpx

DEFAULT_MODEL = "anthropic/claude-haiku-4-5"
DECISIONS_POINT = "docs_agent"
DECISIONS_QUESTION_SET = "docs_agent_v1"
INSPECT_URL = os.environ.get("MIKE_INSPECT_URL", "http://127.0.0.1:9989")
_TIMEOUT = 120
_DECISIONS_TIMEOUT = 10


def decide(state: dict, questions: dict, point: str = DECISIONS_POINT) -> dict | None:
    """Ask Jev typed questions about `state` through mike's decisions route, so
    the call is recorded in decisions.records like every other decision point.

    Returns the `answers` object when Jev decided, or None when the decisions
    layer is off for this point, mike cannot be reached, or the call failed, so
    the caller falls back to the chat-completions path. Never raises."""
    try:
        resp = httpx.post(
            f"{INSPECT_URL.rstrip('/')}/api/decisions/ask",
            json={
                "point": point,
                "question_set": DECISIONS_QUESTION_SET,
                "state": state,
                "questions": questions,
            },
            timeout=_DECISIONS_TIMEOUT,
        )
        if not resp.is_success:
            print(f"  mike decisions error {resp.status_code}: {resp.text[:300]}")
            return None
        body = resp.json()
    except (httpx.HTTPError, ValueError) as err:
        print(f"  mike decisions call failed: {err}")
        return None
    if not isinstance(body, dict) or body.get("decided_by") != "jev":
        return None
    answers = body.get("answers")
    return answers if isinstance(answers, dict) else None


def complete(system: str, user: str, model: str = DEFAULT_MODEL) -> str:
    api_key = os.environ["OPENROUTER_API_KEY"]
    resp = httpx.post(
        "https://openrouter.ai/api/v1/chat/completions",
        headers={
            "Authorization": f"Bearer {api_key}",
            "HTTP-Referer": "https://github.com/temet-ai/mike",
            "X-Title": "temet-proxy docs-agent",
        },
        json={
            "model": model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0.1,
        },
        timeout=_TIMEOUT,
    )
    if not resp.is_success:
        print(f"  OpenRouter error {resp.status_code}: {resp.text[:500]}")
    resp.raise_for_status()
    return resp.json()["choices"][0]["message"]["content"].strip()
