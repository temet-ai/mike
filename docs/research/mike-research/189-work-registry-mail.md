---
title: "#189 work-registry mail: no sender, stale burst"
status: active
created: 2026-11-15
updated: 2026-11-15
summary: Registry mail (and five other mike workers' mail) has no sender row and renders as "an unknown sender"; a staleness bound already exists and held since 11-13, so the live defect is the sender and same-tick bursts.
session_id:
  - f2ff2215-ee7a-4c56-80de-217451dbcb1e
---

# #189 work-registry mail: no sender, stale burst

Data: capture DB `mailbox.messages` joined to `agents.work_item_attention` on `delivered_message_uuid`, rows queued in the last 7 days (2026-11-08 to 2026-11-15), read-only.

## What is true now

**Why the header says "an unknown sender".**
- `deliver_work_item_attention` inserts the mail with `from_session_id = NULL` (`crates/mike-launch/src/store/persistence/work_item_attention.rs:87`, the `INSERT ... SELECT $1, a.session_id, NULL, $2`). The sender is a body prefix, `[mike work registry] ` (`crates/mike-launch/src/work_registry_worker/delivery.rs:179`).
- The header is built by `MailboxEnvelope::letter_as` (`crates/mike-launch/src/store/persistence/mailbox/types.rs:99-107`): a label wins, else `session <uuid>`, else `UNKNOWN_SENDER` (`crates/mike-base/src/wire/mail_text.rs:2`).
- `MIKE_SENDER` ("mike", `mail_text.rs:3`) is passed as the label in one place only: `brief_reminder` (`crates/mike-launch/src/launch/brief_delivery.rs:18`), the launch brief push.
- Stored mail is rendered by `ClaimedChannelMessageRow::envelope` (`types.rs:247-252`) with `from_label`. `from_label` is only derived from `from_session_uuid` (`crates/mike-fleet/src/store/persistence/mailbox/channel.rs:84-87`; `sender_label` in `fleet_seat.rs:132`). A row with no sender can never get a label, so every NULL-sender row renders as unknown.

**It is wider than the registry.** In 7 days, 951 mails start `[mike <word>]`, and all 951 have `from_session_id IS NULL`:

| prefix | mails |
|---|---|
| `[mike work registry]` | 680 |
| `[mike judge]` | 86 |
| `[mike reference]` | 79 |
| `[mike check]` | 76 |
| `[mike question]` | 17 |
| `[mike mail]` | 13 |

(The issue's comment names `[mike mail]`; `[mike gate]`, `[mike floor]`, `[mike tripwire]` etc. exist in code, `crates/mike-fleet/src/launch/fleet/`, with no rows this week.)

**Registry volume, 7 days.** 680 mails, 67 recipients, all NULL sender. Per day: 37, 169, 35, 55, 59, 83, 206 (11-14), 36 (11-15 so far). Kinds: `stalled_wait` 328, `main_idle_with_open_work` 291, `result_lost` 57, `agent_silent` 4. Top recipients get 58-60 mails each in the week. 95 session/kind pairs were mailed more than once.

**Delivery outcome.** 639 consumed over the channel, 15 sent by request injection, 26 dead-lettered (never consumed; 25 of them `stalled_wait`/`main_idle_with_open_work`).

**Burst sizes** (mails per recipient per minute): 1 mail in 76 minute-buckets, 2 in 254, 3 in 18, 4 in 8, 10 in 1. 274 recipient-seconds carry more than one mail; they hold 589 mails, so one mail per recipient per tick would remove 315 of 680 (46%). Grouping per recipient per 5 minutes would remove 330 (49%). Most bursts are pairs: `stalled_wait` and `main_idle_with_open_work` for the same session, which is one problem told twice.

**The stale burst of the issue.** 2026-11-13 08:43:39.638753 UTC, recipient session db id 2152809: 10 mails in one timestamp, 9 `result_lost`/`stalled_wait`/`main_idle` items aged 21.0 to 23.5 hours. That is the only delivery over 2 h old in the week (age at delivery: median 15.1 min, p90 16.0 min, p99 1275 min, max 1410 min; buckets <5 min 42, 5-30 min 627, 30 min-2 h 1, >12 h 10).

**A staleness bound already exists and has held since.** `DELIVER_RECENT_HOURS = 2` (`crates/mike-launch/src/work_registry_worker.rs:26`); `decide` returns `SettleStale` for an unmailed pending item older than that when delivery is on (`delivery.rs:40-45`); `registry.rs:226-228` settles it. Mail also rescans and settles items no longer true (`registry.rs:231-255`). Evidence it works: since 11-12, 7 attention rows were settled `stale` (3 on 11-12, 4 on 11-13), 76 `resolved`; no delivery older than 2 h after 11-13 08:43. The burst happened while delivery was off or before the bound landed (`work_registry.wake_mode` is now `deliver`; the mode is read through `WAKE_MODE_KEY`, `delivery` is on only when it equals "deliver", `registry.rs:169-183`). So cause 2's "stale" half is fixed; the "one mail per item, all at once" half is not.

**One mail per item is by design today**: `act_on_session` loops items and calls `delivery::mail` for each (`registry.rs:219-238`), and `attention_message_uuid` derives a message uuid per attention id (`delivery.rs:106-111`), so each item is its own mailbox row and its own wake.

## Options compared

### Sender

| Option | Work | Fixes | Leaves broken |
|---|---|---|---|
| A. Pass a label, like `MIKE_SENDER` | Header comes from a constant; needs a label to reach the claim path. Today the claim path only sees a label derived from `from_session_uuid`, so a NULL-sender row still gets none. A label constant alone fixes only mail rendered at push time (`render_as`), not stored mail. | Brief pushes (already done) | The 951 stored rows: they are read back through `envelope()` with no label |
| B. A `sender_label` column on `mailbox.messages`, read into `from_label` | One migration (nullable text), the registry insert sets `'mike work registry'`, the claim query selects it, `envelope()` uses it (`types.rs:251`). The other five workers pass their name the same way. Body prefix dropped. No backfill needed: old rows keep the unknown header, new ones are named. | All 951/week at once, each by its own name; readers see the name without parsing the body; no fake session | One column added to a hot table (small, nullable) |
| C. A system session id for mike | Create a sessions row, set `from_session_id` on every mike worker mail. | Header gets a session label | A fake session shows in roster, thread, reply routes and "mail from session X" toasts; replies go nowhere; every reader that joins sender gets a phantom |

B is the one that needs no phantom session and covers all six prefixes. A alone is not enough for stored mail.

### Burst

| Option | Effect on 7-day data | Risk |
|---|---|---|
| 1. One mail per session per tick listing the items | Removes 315 of 680 mails (same-second), 330 with a 5-minute window; 10 to 1 for the worst burst. One wake per tick. | Item ids must all be marked delivered together; one `message_uuid` per tick, so retry/`given_up` per item changes |
| 2. A staleness bound | Already exists at 2 h; caught 7 items this week and nothing over 2 h has been delivered since 11-13. A tighter bound would not reduce the 46% of same-tick mails, because their ages are mostly under 30 min (627 of 680). | Tightening drops notices still useful |
| 3. Settle items whose agent ended | The 10-burst items were `result_lost` ("job still running when agent ended"): their agent had ended by definition, so settling these would silence the notice they exist to send. Applies only to `stalled_wait`/`main_idle` repeats: 619 of 680 mails, and 95 session/kind pairs repeat. | A `stalled_wait` for an ended session is moot; needs the "session ended" signal the registry already has (`rescan`, `registry.rs:231-235`) |

## Recommendation

1. **Sender (B).** Add a nullable `sender_label` column to `mailbox.messages`; set it in the registry insert (`work_item_attention.rs`, `'mike work registry'`) and for `[mike judge|reference|check|question|mail|gate|floor|tripwire]`; select it into `from_label` in `channel.rs`; delete the `[mike work registry] ` body prefix (`delivery.rs:179`). Header reads `Mail from mike work registry: ...`. No system session. Do this first: it is 951 mails/week and is what tether and every other reader shows.
2. **Burst (1).** One mail per session per pass listing the items, one message uuid per pass. Keep the existing 2 h bound (option 2 is already done; do not tighten it). Settle `stalled_wait`/`main_idle_with_open_work` for a session whose agent or main thread ended (option 3), but never `result_lost`.

## Measure

Read from the capture DB after the fix deploys; each is one query:
- Mail with no sender: `count(*) where body ~ '^\[mike ' or sender_label is null and from_session_id is null` per 7 days: 951 now, goal 0.
- Mails per recipient per tick: `max` per recipient-second, 10 now (2 for the median burst), goal 1; and registry mails/week 680 now, expect about 365 or fewer.
- Stale notices delivered: `count(*) where queued_at - since > 2 hours`, 10 now (all 11-13), goal stays 0.
- Dead-lettered registry mail: 26 now, expect to fall as moot repeats are settled.
