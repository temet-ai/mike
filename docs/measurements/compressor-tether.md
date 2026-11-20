---
title: File-read compression in sessions running tether and the mods
status: active
created: 2026-11-01
updated: 2026-11-01
summary: Live forwarded sessions with the tether mod loaded. One fault found and fixed (c3f46964, the per-request id in the recovery reference broke byte-identity of re-applied cuts). Checks 1, 2, 3 (re-read with an offset), 4 and the agent.start half of 5 pass. Still open: the word guard's cut-and-continue, a forwarded shell cut, and the file_unchanged path.
session_id:
  - 197a2c57-cd0e-4d13-b75f-d0186cb92fab
---

# File-read compression in sessions running tether and the mods

Measured on 2026-11-01 with Claude Code 2.1.311, using `claude -p --model sonnet` through mike
(`ANTHROPIC_BASE_URL=http://127.0.0.1:9988`). The mods came from `CLAUDE_CODE_PLUGIN_DIRS`:
fleet-comms, mike-mail, mods and tether. Each probe session was forced into the forward arm by
choosing a `--session-id` that hashes under the live 20% share
(`sha256(id)[0..8] % 100 < 20`, see `compress/stage.rs` `session_arm`). No setting was widened.

| Session | Where | Tether |
|---|---|---|
| S1 `89ca792f-5fe0-4e0d-9eff-6aaf358b000a` | scratch repo | shared :9970 |
| S2 `6c352de3-0134-4976-a5bf-7bb49cd1b971` | temet-proxy, read `docs/LAUNCH_DOMAIN.md` (45 KB) | shared :9970 |
| S3 `57d37e70-b45d-49a3-9bc1-d196f0a9df2c` | scratch repo | in-memory :9971, word guard set to the made-up word `quillorbat` |

The cache gate (a9e6a31c) makes a new cut only on a cold cache. Claude Code's main chat sends
`cache_control` with `ttl: 1h`, so each cut needs more than an hour of idle time, plus the hour
after a proxy restart (34e0f026). S1 and S3 produced no cuts, because their files sit under
`~/.claude/.cache/…` and match the protected glob `**/.claude/**` (`skipped: protected_path`).
Every cut in this record comes from S2.

## Where the session's own additions sit

On 2.1.311, tether's `PostToolBatch` git read hint, the rule reminders (`Contents of
.claude/rules/…`), the Read truncation notice and the `prompt.submit` context each arrive as their
own `role: "system"` message. None of them is inside a Read `tool_result` string.
`enforce_read_tool_result_boundary` limits a splice to an enabled producer's tool_result content,
so a cut cannot reach these additions.

## Fault: the re-applied cut changed on every request (fixed, c3f46964)

The marker's recovery reference included `CapturedResults.request_id`. That value was the
provisional `RequestId::next()` from `run_request_pipeline`, not the database id. Live:

| Request | Marker reference | cache_read | cache_creation |
|---|---|---|---|
| 732076 (first cut, cold) | `{"id":401,…}` | 11,005 | 40,543 |
| 732077 (same cut re-applied) | `{"id":402,…}` | 45,966 | 5,794 |

The two sent bodies first differed at `messages[3]`, the cut block. `get_request_body id=402`
returned not found. The fix makes the reference `{tool_use_id, sha256}`. `get_request_body`
(REST `GET /api/tool-results/{tool_use_id}/original`, and the MCP tool without `id`) finds the
first capturing request through `tools.uses` → `tools.results`, in 0.085 ms against a 50 ms read
budget. New tests show that the file-read and shell-output cuts are byte-identical on a later
request. Deployed at 16:50:51Z (build `c3f46964`, clean). compressor-shell-output confirmed the
same behaviour for shell cuts (732280).

## Checks

| # | Check | Result | Evidence |
|---|---|---|---|
| 1 | A cut leaves tether's and the mods' additions untouched | pass | On 732076, 732077, 732610, 732613 and 732614, the only message the compressor changed is the Read tool_result. All `role: system` messages (7–8 per request: hook context, git hint, rule reminders, truncation notice) are byte-identical to the captured original. The other differences are mike's filter-rule stage on `system`, which is the same on every request. |
| 2 | Cache reads stay high after a cut | pass, after the fix | After the deploy: 732610, the cold first cut, read 11,005 and created 42,673. 732613 then read 53,678 and created 1,123, and 732614 read 54,801 and created 515. Between 732610 and 732613 the sent bodies differ only at messages 23 and 25. The client changed those itself (Claude Code moves `cache_control` and normalises the last system message), and the captured originals differ there too. |
| 3 | Recovery | pass, with an offset | In S2 the model saw the `[[mike:file_read_reduced]]` marker, re-read the hidden line with `Read offset=250 limit=1`, and quoted it correctly. Unprompted, it also took back an earlier answer drawn from lines it had never been shown. The `file_unchanged` reply to a full re-read was not tested. |
| 4 | tether reconcile in a compressed session | pass | S2: 8 of 8 mod steps matched one to one. The only wire-only request was one `count_tokens_sidecar`. |
| 5a | A subagent's agent.start context | pass | A `db-dev` subagent started in S2 after the cut (732611 and 732612) received its mike lessons verbatim. `general-purpose` has no lessons, so mike returns `text: null` for it, which is the expected behaviour. |
| 5b | The word guard's cut-and-continue | not done | The shared tether has no words configured, and mike's `weasel_intercept` is off. The rule did reach S3's system prompt from the probe tether (732232 and later). No cut was provoked. |
| — | Forwarded shell-output cut beside the git hint | not done | `shell_output_compression` applies to new sessions. S3 started before it was switched on, so no shell stage ran for S3. |

## Still to run

S4 `93d81a65-fb2e-4769-abd1-164d952ed31e` hashes into the forward arm for both reducers. To finish
the record, run S4 in temet-proxy against the :9971 tether with `quillorbat` set:

1. One turn that reads a large doc, runs a large `git log -p`, and runs `git status`. Then three
   filler turns.
2. Wait more than an hour. Then ask for a hidden line through a full re-read, which exercises
   `file_unchanged`. In the same turn, try to provoke the word guard, and confirm the shell cut
   and the git hint survive.
3. One follow-up turn, to check byte-identity and cache reads.
