"""OpenRouter API client — thin wrapper around the chat completions endpoint."""

import os
import httpx

DEFAULT_MODEL = "anthropic/claude-haiku-4-5"
DECISIONS_MODEL = "typesafe/jev-1.14"
DECISIONS_URL = "https://openrouter.ai/api/alpha/decisions"
_TIMEOUT = 120
_DECISIONS_TIMEOUT = 10


def decide(state: dict, questions: dict, model: str = DECISIONS_MODEL) -> dict | None:
    """Ask OpenRouter's decisions endpoint typed questions about `state`.

    Returns the `answers` object, or None on any failure so the caller can fall
    back to the chat-completions path. Never raises."""
    api_key = os.environ.get("OPENROUTER_API_KEY")
    if not api_key:
        return None
    try:
        resp = httpx.post(
            DECISIONS_URL,
            headers={
                "Authorization": f"Bearer {api_key}",
                "HTTP-Referer": "https://github.com/temet-ai/mike",
                "X-Title": "temet-proxy docs-agent decisions",
            },
            json={"model": model, "state": state, "questions": questions},
            timeout=_DECISIONS_TIMEOUT,
        )
        if not resp.is_success:
            print(f"  OpenRouter decisions error {resp.status_code}: {resp.text[:300]}")
            return None
        answers = resp.json().get("answers")
    except (httpx.HTTPError, ValueError) as err:
        print(f"  OpenRouter decisions call failed: {err}")
        return None
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
