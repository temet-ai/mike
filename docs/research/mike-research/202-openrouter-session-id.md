---
title: "#202 OpenRouter session_id for prompt-cache sharing"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: No caller uses OpenRouter today; cached tokens are recorded on the OpenRouter path but not on the Jev path. Recommend a body session_id of role plus system-prompt hash, measured by cached share per role in llm.calls.
session_id:
  - 2255aa38-c903-473b-bed0-929d9f0e25e7
---

## What is true now

**Callers on OpenRouter: none.** Every `llm.calls` row of the last 14 days (16,109 rows, all `ok`) is `claude/sonnet`, `claude/haiku` or `typesafe/jev-1.14-20261014`. The settings at `/api/settings` set every role to `claude/sonnet` or `claude/haiku`; `steerer_openrouter_configured_via_env` is true, so a key is present. The last OpenRouter-routed rows ended 2026-10-13 (`google/gemini-3.8-flash`, `openai/gpt-5.6-luna`), then roles moved to `codex/gpt-6-sol` (to 10-26) and `claude/*`.

**No session_id is sent.** `crates/mike-base/src/llm/openrouter/call.rs:109-126` (`build_body`) sends model, messages, `response_format` and `provider.require_parameters` only. `send_raw` (`call.rs:140-160`) sets only `HTTP-Referer` and `X-Title`. Resends (`call.rs:194-224`) reuse the same body shape, so one id would cover them.

**Cached tokens are recorded, with one gap.**
- OpenRouter path: `extract_usage` reads `usage.prompt_tokens_details.cached_tokens` (`call.rs:45-49`); `recording.rs:47` writes it to `llm.calls.cached_tokens` (column from migration `20260829000002_llm_calls.sql:48-51`).
- Claude CLI path: records `cache_read_input_tokens` (`provider_tests.rs:113`).
- Jev path: `jev_call.rs:45` and `jev_call.rs:289` hard-code `cached_tokens: None`. All 3,207 `decision` rows in 14 days have it null.

**What the 14-day data says** (`outcome='ok'`; prompt tokens are the sum, cached share = cached/prompt):

| role / model | calls | avg prompt | cached share |
|---|---|---|---|
| steerer claude/sonnet | 4,060 | 4,070 | 53% |
| judge_worker claude/sonnet | 3,754 | 3,725 | 51% |
| session_judge claude/sonnet | 1,286 | 6,774 | 49% |
| eval claude/sonnet | 961 | 2,855 | 39% |
| lesson_proposer claude/sonnet | 931 | 6,419 | 20% |
| lesson_relations (sonnet+haiku) | 170 | ~2,000 | 55-59% |
| decision typesafe/jev | 3,207 | 695 | not recorded |

Total about 48.4 M prompt tokens in 14 days, about 3.5 M a day.

**The only OpenRouter evidence (all-time, no session_id sent):**

| model / role | calls | cached share |
|---|---|---|
| openai/gpt-5.6-luna steerer | 8,526 | 8.1% |
| google/gemini-2.5-flash-lite session_judge | 44,956 | 4.2% |
| google/gemini-3.5-flash-lite session_judge | 2,959 | 0.5% |
| google/gemini-3.8-flash judge_worker | 9,732 | 0% |

So without a session_id the OpenRouter callers cached 0-8%, against about 50% on the Claude CLI with the same prompts. That gap is the room the id has to close.

**The intake-criteria author cannot be seen.** `LlmRole` (`crates/mike-base/src/llm/usage.rs:35-45`) has no intake role, and no role matches the issue's 3,260 input / 704 output averages. Its calls are not separable in `llm.calls`.

**OpenRouter docs (fetched 2026-11-15, prompt-caching guide).**
- `session_id` goes in the body or in the `x-session-id` header; "If both are provided, the body value takes precedence." Maximum 256 characters. Without either, `prompt_cache_key` is the fallback.
- With a session_id, sticky routing starts on any successful request; without one it starts only after a cache hit is seen. It applies only when the provider's cache read is cheaper than normal input. Sessions expire after 60 minutes without a request.
- DeepSeek: automatic caching, writes at input price, reads at 0.1x. The guide states no minimum token count.
- Embeddings, rerank, speech, image and video accept only the header; mike's chat completions path is not one of these.

## Options compared

| | A. role + hash of system prompt (the issue's diff, plus role) | B. caller name only | C. mike session id |
|---|---|---|---|
| Ids | one per caller prompt; 6 roles today | one per role | one per session; about 3.5 M tokens/day spread over hundreds of ids |
| Prompt edit | new id, cold cache once; correct, since the prefix changed | same id, but the cache is cold anyway | n/a |
| Cache sharing | every call of one prompt shares a provider | same | splits the shared system prompt per session; each session pays its own warm-up, so lower hit rate |
| Work | `build_body` takes the role (`self.inner.role` is already there); about 10 lines and a test | about 5 lines | needs a session id threaded in; `call_context.session_id` is a bigint row id, not a stable external id, and background calls often have none |
| Risk | all calls of one prompt pin to one provider while warm; fine at 3.5 M tokens/day | same | none beyond worse hit rate |

Header against body: the body field wins when both are present and `build_body` already builds the JSON, so the body is one line; the header would need a change in `send_raw` and gains nothing. Use the body.

**Saving.** Let P be the OpenRouter input price per million tokens for the chosen model. I have not looked up a current DeepSeek price, so the saving is a rate, not a dollar figure. Today's 3.5 M tokens/day with the Claude-observed 50% stable prefix puts 1.7 M tokens/day in the cacheable part. If the id lifts the OpenRouter cached share from the observed 0-8% to the 50% the CLI path gets, the extra cached tokens are about 1.5 M/day. At 0.1x that saves 0.9 x P x 1.5 = about 1.35 x P dollars per day (at P = $0.14/M, about $0.19/day). The issue's own replay shows output was most of DeepSeek's cost, so the saving is small in money; the argument for the id is correctness of routing and cost that stays flat as volume grows, not a large bill cut. This is an upper bound on a hypothetical, since no caller is on OpenRouter now.

## Recommendation

1. Do A, in the body: `"session_id": "mike-<role>-<first 16 hex of sha256(system)>"`. Prefix with the role so two callers sharing a system prompt text still read apart in the OpenRouter dashboard.
2. Before relying on a measure, fix the Jev path to record cached tokens (`jev_call.rs:45`, `:289`) if its response carries them; otherwise state that `decision` rows are unmeasured.
3. Do not ship it ahead of a caller on OpenRouter: it changes nothing until one is. Pair it with the first role moved to an OpenRouter model, so the measure has a before and an after.
4. Keep `provider` free of `sort` and `order`, as the issue says.

## Measure

Cached input share per role and model in `llm.calls`: `sum(cached_tokens)::float / sum(prompt_tokens)`, over `outcome='ok'` rows of OpenRouter models, grouped by `role, model`, read before and after the id ships. Baseline from above: 0-8%. Pass: at least 40% for steerer and judge_worker after the first warm hour (the Claude CLI gets 51-53% with the same prompts). Also check that `cached_tokens` is non-null for 100% of OpenRouter rows, and that `decision` rows stop being null if step 2 lands.
