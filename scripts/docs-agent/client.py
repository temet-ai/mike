"""OpenRouter API client — thin wrapper around the chat completions endpoint."""

import os
import httpx

DEFAULT_MODEL = "anthropic/claude-haiku-4-5"
_TIMEOUT = 120


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
