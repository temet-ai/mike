---
title: Docs-agent pattern
status: active
created: 2026-06-28
updated: 2026-11-20
summary: How the docs-agent GitHub Actions workflow classifies a diff and updates documentation automatically via OpenRouter, and the reusable step pattern behind it.
---

# Docs-agent pattern — mike

A GitHub Actions workflow updates documentation when code lands on main, using OpenRouter
(a Haiku model) as the LLM backend.

## Files

```
.github/workflows/docs-agent.yml   — triggers on push to main, skips [docs-agent] commits
scripts/docs-agent/
  __main__.py                       — orchestrator: classify → run steps → commit → PR
  client.py                         — OpenRouter chat completions (one function: complete())
  git_util.py                       — git diff/read/write/commit/push/create_pr helpers
  requirements.txt                  — httpx only
  steps/
    analyze.py                      — Step 0: classify diff → JSON flags per category
    architecture.py                 — Step 1: ARCHITECTURE.md, REQUEST_FLOW.md, STARTUP_FLOW.md
    schema.py                       — Step 2: SCHEMA.md (triggered by migrations/)
    guides.py                       — Step 3: TROUBLESHOOTING.md, dev guides
    ui_help.py                      — Step 4: docs/frontend/ROUTES.md (triggered by routes/)
```

## The pattern (reusable)

Each step follows the same shape — copy this when adding a new one:

1. **Scope the diff** — `git_util.scope_diff_to(diff, "some/prefix/")` — only send relevant hunks
2. **Read current content** — `git_util.read("path/to/doc.md")`
3. **Prompt the LLM** — system prompt with strict rules, user prompt with current content + diff
4. **Expect `NO_CHANGES` or the full updated file** — no diffs, no patches, whole file back
5. **Write if changed** — `git_util.write(path, result)`

The analyze step returns JSON flags (`architecture`, `schema`, `guides`, `ui_help`) so downstream
steps only fire when relevant. Add a new flag to the JSON and a new step file to extend.

## OpenRouter setup

- Model: `anthropic/claude-haiku-4-5` (`DEFAULT_MODEL` in `scripts/docs-agent/client.py`)
- Auth: `OPENROUTER_API_KEY` repo secret
- The `client.py` wrapper is ~25 lines. OpenRouter's API is OpenAI-compatible; pass
  `HTTP-Referer` and `X-Title` headers for dashboard attribution.
- `anthropics/claude-code-action` (Anthropic's official GHA) requires the Anthropic API directly —
  this pattern deliberately avoids it so OpenRouter credits can be used instead.

## Key rules baked into each step prompt

These prevented hallucination in practice — keep them:

- "Only correct facts that the diff makes wrong. Never rewrite content that is still accurate."
- "Never add historical narrative: no 'previously', 'used to', 'now does X instead'."
- "Describe what the system IS, not how it got there."
- "If nothing needs changing, return exactly: NO_CHANGES"
- "Return the complete updated document (not a diff or patch)."

## Secrets needed

| Secret | Where | Purpose |
|--------|-------|---------|
| `OPENROUTER_API_KEY` | Repo → Settings → Secrets → Actions | LLM calls |
| `GITHUB_TOKEN` | Auto-provided by Actions | `gh pr create` |

The workflow needs `permissions: contents: write` and `pull-requests: write`.
