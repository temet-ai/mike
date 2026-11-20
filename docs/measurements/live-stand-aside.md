---
title: Live check: mike stands aside for the stages tether takes
status: done
created: 2026-10-28
updated: 2026-10-31
summary: Live claude -p sessions through the deployed proxy with the tether mod and every client_serves toggle on. 10-28 covered identity, briefing, lessons and tools; 10-31 covered recall, findings, agent lessons, mail and the word guard, found four faults, and re-ran them fixed on 3910f641.
session_id:
  - 63f7c846-e8b8-4f3b-8047-0877ad3fc650
  - 2031f048-bff8-4f7e-9e0e-1a735f9a01c8
---

# Live check: mike stands aside for the stages tether takes

Deployed mike `879d5ecd` (clean build) and tether `8f97fbd` (`scripts/install`, 2026-10-28 10:18).
Every `client_serves_*` toggle was turned on through `PATCH /api/settings/features/{key}`.
One `claude -p --model haiku` session, `7689af1a-1871-4766-8ae4-e2f8bf49cf19`, run in this
checkout through `ANTHROPIC_BASE_URL=http://127.0.0.1:9988` with the tether mod loaded, made one
subagent call. The model answered with its own session id, so the identity text reached it.

## What mike recorded

From `GET /api/requests/{id}/manifest`. "tether took it" means the stage stood aside for this session.

| Stage | 717619 (primary_first) | 717624 (subagent) | 717628 (primary) | Who delivered |
|---|---|---|---|---|
| filter | `filter_served_by_client` | `filter_served_by_client` | `filter_served_by_client` | tether took it |
| lazy_sections | `lazy_sections_served_by_client` | same | same | tether took it |
| composer | `composer_served_by_client` | same | same | tether took it |
| lessons | `session_lessons_held_by_client` | | same | tether took it |
| self_identity | `self_identity_served_by_client` | | | tether; the text is in the sent body |
| workspace_briefing | `workspace_briefing_already_delivered` | | | tether; the text is in the sent body |
| change_record_tail | `change_record_tail_served_by_client` | | | tether took it; nothing had changed, so no text |
| proxy_tools | `proxy_tools_session_has_them` | same | same | tether; `mike_session_roster` is in the sent tools |
| recall_injector | `recall_no_tags` | same | same | not exercised: the model asked for no topic |
| reminders (findings) | `diagnostic_reminder_nothing_to_inject` | | same | not exercised: no findings |
| agent_lessons | | `agent_lessons_nothing_to_inject` | | not exercised: no lessons for `general-purpose` |
| steerer (mail) | `steerer_queue_empty` | | same | not exercised: no mail |
| reminder_on_token, git_read_hint, repeated_notices, read_dedup, file_read_compression | nothing to do | | | stay in the proxy |

The forwarded body of 717619 holds `proxy-self-identity`, `proxy-workspace-briefing` and the
`mike_session_roster` tool, and mike injected none of them, so tether delivered all three. No
lesson text reached the session from either side. With `mod_lesson_delivery` on, mike leaves
lessons to the client, so a session whose client sends none gets none. Whether tether had no
relevant lessons for this prompt or failed to send them is not settled by this run.

## Not yet seen live

Recall, analyzer findings, agent lessons, mail and the weasel guard had nothing to do in this run.
The 2026-10-31 run below covers all five.

## 2026-10-31: the five stages not seen on 10-28

Deployed mike `2e913b37` and tether `4ca17ee`, every `client_serves_*` toggle and
`mod_lesson_delivery` on. Session `f90d65b5-a5ae-4db3-a86c-b780bff3fece` (`claude -p --model
haiku`, four turns): a `db-dev` subagent, a `<mem:recall topic="project-controls"/>` tag, a
mail sent between turns, "for now" in an answer, and tether's word guard set to
`comprehensive` for the run (cleared afterwards).

| Stage | What happened | Verdict |
|---|---|---|
| recall | tether queued `recall:…:95d1e3b8`; the next prompt quoted lesson 1853; mike recorded `recall_served_by_client` | works |
| agent lessons | tether gave `db-dev` its lessons (the text is in subagent request 726381's body); mike recorded `agent_lessons_nothing_to_inject` because it could not resolve the persona | works; manifest label wrong |
| mail | tether delivered it (the model answered `MAIL-RECEIVED-7Q`), but at 18:54 the reaper told the sender it was "never delivered": `/mail/claim` set only `claimed_at` | fault |
| word guard | tether cut `comprehensive` and the model got `mcp__tether__correction`; mike logged "weasel intercept stands aside" | works |
| findings | mike stored "for now" on 726416, 726420 and 726450; nobody delivered them: `latest_findings` read a request `snapshot_terminal` had already removed from memory | fault |
| self identity | tether's `/context` call missed its 300 ms budget (tether log 18:43:41), but mike had marked identity served, so request 726379 went out with no identity | fault |
| workspace briefing | same race; mike injected it itself on 726379 | fault |

Fixed in `3910f641`: findings are kept per session at analysis time; identity and briefing
count as served only when the client's text is in the request body; a client mail claim
stamps the message sent. Re-run on the deployed `3910f641`, session
`5581d5a5-a1fd-41e6-9457-a41ecad7c82f`:

| Request | Evidence |
|---|---|
| 726987 (first) | identity line in the client's body; `self_identity_served_by_client`, `workspace_briefing_already_delivered`; finding "for now" stored |
| 726990 (next prompt) | `reminders_served_by_client`; "anti-patterns flagged" text in the client's body (tether intent `mike:reminders:e85552510313e840`) |
| 726990 | mail `23e3c140…` in the body, model answered `MAIL-OK-K4`; the row has `claimed_at` and `sent_at` |

Two causes remained after this re-run, both fixed below. A `/context` or `/mail/claim` call
that missed tether's budget lost what mike consumed for it (the change-record cursor, a
claimed mail). The routes missed it because Postgres COMMITs took 2.2–3.7 s from 18:44:09 to
18:44:13 UTC and each route awaited the pool several times in a row.

### Final run on `970b9118` (mike) and `277aa14` (tether)

Includes mike `ab4cf4f2` (the client routes answer inside tether's budgets while Postgres
stalls) and `bca43939` (agent-lessons persona and manifest), and tether `0d97fb0` (late
answers kept for the next call) and `277aa14` (loaded-mods note retried once mike knows the
session). Session `d1f7f41b-22f1-4d6d-a29d-8dd72c0fbde3`:

| Request | Lane | Evidence |
|---|---|---|
| 727699 | first | `self_identity_served_by_client` with the identity line in the body; `workspace_briefing_already_delivered` |
| 727701 | db-dev subagent | `agent_lessons_suppressed_served_by_client`; db-dev's lesson text in the body |
| 727709 | next prompt | `reminders_served_by_client` with the "for now" finding in the body; mail `88d0829e…` in the body (model answered `MAIL-FINAL-Z9`); the row has `claimed_at` and `sent_at`, not dead-lettered |
| notes | | two `Loaded mods: 5` notes, one per `claude -p` process |

Every stage tether takes was seen delivered by tether and stood aside by mike, with nothing
missing or delivered twice.

### Mail consumed, shaping removed: mike `2fdb294c`, tether `b7a3806`

Three test mails tether had delivered on 2026-10-31 (`23e3c140`, `ca1fb880`, `88d0829e`) were
dead-lettered about an hour later and their senders told "never acknowledged": `3910f641`
stamped a claimed mail sent but left it unconsumed. mike `22978dc0` records a client-claimed mail
consumed. tether's shaping feature, which no stored rule could ever match, is removed (tether
`b7a3806`, mike `22978dc0`). Session `42aa0fba-14e7-403b-87f2-4341039a2305`:

| Check | Evidence |
|---|---|
| mail | `7203bfa8…` claimed, sent and consumed, not dead-lettered; the model answered `MAIL-CONSUMED-Q7` |
| filter rules in a tether session | 728692 `filter_rule_matched`; 728700 and 728703 `filter_replay_applied` |
| tether no longer reads `/prompt-shaping` | no prompt-shaping request in tether's log for the run |
| `--fork-session` resume | new session `554c46bb…`: request 728720 carries its own identity line from tether, `self_identity_served_by_client` |

### First-request filter rewrite broke the prompt cache: fixed live on `fef0b26e`

When a filter rule fired on a session's first request, mike re-serialised the whole `system`
array, re-sorting each block's keys, while later requests replayed only the changed spans.
Request 2's prefix then differed from request 1's: live 728894 read 15,682 cached tokens and
created 36,396. `a4c488ae` and `fef0b26e` splice only the items a rule changed or appended.
Session on `fef0b26e`: requests 729215 and 729216 send byte-identical `system` blocks, and 729216
read 46,586 cached tokens and created 1,520.

### Injected reminders replayed: fixed live on `f2e988e1`

A block mike appends to a user message (lessons, identity, briefing, findings, recall, mail) is
not in the client's history, so the session's next request diverged inside that message and
rebuilt the cache from there. Over the 7 days to 2026-10-31, 76 requests right after an injection
created more than 40k cache tokens each, 14.2M in total. `f2e988e1` splices each injected block
back into its message on later requests when the message is unchanged. Session
`fb90a1af-14c3-42c1-956a-b50fd772fc11` (lesson control arm, so mike injects): 729325
`session_lessons_injected`; 729326 `injected_block_replayed`, read 46,363 cached tokens and
created 936 (the same case before the fix, 729236, created 44,954).
