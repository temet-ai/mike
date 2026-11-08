#!/usr/bin/env bash
# Weekly Claude usage audit against mike's capture store.
#
# Answers, without a model in the loop, the questions that took a full
# interactive session to re-derive on 2026-09-20:
#   - where the week's tokens went, by session and by model seat
#   - whether the temet-ai/mike-private#252 cache re-write fix is still holding
#   - which steering rules have gone dead (anchored to text that changed)
#   - which projects have accumulated turns but no reachable lessons
#   - how much of the spend mike can actually attribute to an operator turn
#
# Writes a dated report and updates `latest.txt`. Exits 1 if any alarm trips;
# the proxy's scheduler (the usage_audit job) records exit 1 as a finished run
# with alarms and shows the alarm lines. Runs on Linux and macOS.

set -euo pipefail

DAYS="${AUDIT_DAYS:-7}"
OUT_DIR="${AUDIT_OUT_DIR:-$HOME/.local/state/mike-audit}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
dotenv_url() { (set -a; . "$ROOT/.env" >/dev/null 2>&1; printf '%s' "${MIKE_DATABASE_URL:-}"); }
PGURL="${AUDIT_PGURL:-${MIKE_DATABASE_URL:-$(dotenv_url)}}"
if [ -z "$PGURL" ]; then
    echo "usage-audit: no database URL: set AUDIT_PGURL or MIKE_DATABASE_URL, or provide $ROOT/.env" >&2
    exit 2
fi

# Alarm thresholds
REWRITE_PCT_MAX="${AUDIT_REWRITE_PCT_MAX:-1.5}"   # pre-#252 range started here
CONTEXT_MEAN_MAX="${AUDIT_CONTEXT_MEAN_MAX:-200}" # thousand tokens per request

# Put a failure in front of the user instead of leaving it in a file nobody
# opens. The path-drift bug that orphaned 96 lessons went unnoticed for eleven
# days precisely because nothing ever said anything out loud.
shout() {
    local title="$1" body="$2"
    [ "${AUDIT_NO_NOTIFY:-0}" = 1 ] && return 0
    command -v omarchy-notification-send >/dev/null 2>&1 &&
        omarchy-notification-send --urgency critical --glyph $'' "$title" "$body" >/dev/null 2>&1 && return 0
    command -v notify-send >/dev/null 2>&1 &&
        notify-send --urgency=critical "$title" "$body" >/dev/null 2>&1 && return 0
    return 0
}

# An audit that dies quietly is worse than no audit: it looks like "no alarms".
trap 'rc=$?; [ $rc -ne 0 ] && [ $rc -ne 1 ] && shout "Claude usage audit failed" "exit $rc — the audit did not run, so today is unmeasured. Check: $REPORT"' EXIT

mkdir -p "$OUT_DIR"
STAMP="$(date -u +%Y-%m-%d)"
EVAL_ONLY="${AUDIT_EVAL_ONLY:-0}"
if [ "$EVAL_ONLY" = 1 ]; then
  REPORT="$OUT_DIR/eval-$STAMP-$(date -u +%H).txt"
else
  REPORT="$OUT_DIR/$STAMP.txt"
fi
SINCE="$(date -u -d "$DAYS days ago" +%Y-%m-%d 2>/dev/null || date -u -v-"${DAYS}"d +%Y-%m-%d)"

q() { psql "$PGURL" -P pager=off -A -F $'\t' --no-psqlrc -v ON_ERROR_STOP=1 -c "$1"; }
# scalar: tuples-only, no header and no "(N rows)" footer
qs() { psql "$PGURL" -P pager=off -At --no-psqlrc -v ON_ERROR_STOP=1 -c "$1" | head -n1; }

# Per-million list prices: input / output / cache-write / cache-read.
PRICES="(VALUES
  ('claude-sonnet-5',3.0,15.0,3.75,0.30),
  ('claude-fable-5-1',15.0,75.0,18.75,1.50),
  ('claude-opus-5',15.0,75.0,18.75,1.50),
  ('claude-haiku-4-5-20251001',1.0,5.0,1.25,0.10)) v(model,pin,pout,pcw,pcr)"

COST="r.input_tokens*px.pin + r.output_tokens*px.pout
    + r.cache_creation_input_tokens*px.pcw + r.cache_read_input_tokens*px.pcr"

eval_present() {
  [ "$(qs "SELECT to_regclass('eval.alarms') IS NOT NULL")" = "t" ]
}

eval_section() {
  echo "== judgement metrics and alarms (eval) =="
  if ! eval_present; then
    echo "eval schema not present"
    return 0
  fi
  echo "-- open alarm episodes"
  q "SELECT kind, subject, first_seen_at, last_seen_at, evaluations,
            acknowledged_at, jsonb_pretty(detail) AS detail
       FROM eval.alarms WHERE cleared_at IS NULL
      ORDER BY first_seen_at DESC;"
  echo "-- newest rollup"
  q "SELECT for_day, sql_version, finished_at FROM eval.rollups
      WHERE finished_at IS NOT NULL ORDER BY for_day DESC, started_at DESC LIMIT 1;"
  echo "-- turn judge accuracy, class corrected, all history"
  q "WITH newest AS (SELECT id FROM eval.rollups WHERE finished_at IS NOT NULL
                      ORDER BY for_day DESC, started_at DESC LIMIT 1)
     SELECT r.metric, r.label_tier, r.label_split, r.label_source, r.k, r.n,
            round(r.value::numeric, 4) AS value, round(r.ci_low::numeric, 4) AS ci_low,
            round(r.ci_high::numeric, 4) AS ci_high
       FROM eval.rates r JOIN newest ON newest.id = r.rollup_id
      WHERE r.family = 'accuracy' AND r.role = 'turn_judge' AND r.class = 'corrected'
        AND r.window_days IS NULL AND r.bundle_label IS NULL AND r.dimension IS NULL
      ORDER BY r.label_tier, r.label_split, r.label_source, r.metric;"
  echo "-- label mix, current against baseline"
  q "WITH newest AS (SELECT id FROM eval.rollups WHERE finished_at IS NOT NULL
                      ORDER BY for_day DESC, started_at DESC LIMIT 1)
     SELECT r.class, r.window_days, r.k, r.n, round(r.value::numeric, 4) AS share
       FROM eval.rates r JOIN newest ON newest.id = r.rollup_id
      WHERE r.family = 'label_mix' AND r.population = 'real_turns'
      ORDER BY r.class, r.window_days;"
}

eval_alarms() {
  eval_present || return 0
  while IFS=$'\t' read -r kind subject first_seen; do
    [ -n "$kind" ] || continue
    alarms+=("eval: $kind $subject open since $first_seen — see /settings/workers")
  done < <(psql "$PGURL" -P pager=off -At -F $'\t' --no-psqlrc -v ON_ERROR_STOP=1 -c "SELECT kind, subject, first_seen_at
      FROM eval.alarms WHERE cleared_at IS NULL AND acknowledged_at IS NULL ORDER BY first_seen_at;")
  local enabled
  enabled=$(qs "SELECT coalesce((SELECT value::text FROM config.settings WHERE key = 'eval.rollup.enabled'), 'true')")
  if [ "$enabled" = "false" ]; then
    echo "eval metrics disabled on /settings" >> "$REPORT"
    return 0
  fi
  local fresh
  fresh=$(qs "SELECT EXISTS (SELECT 1 FROM eval.rollups WHERE finished_at >= now() - interval '36 hours')")
  if [ "$fresh" != "t" ]; then
    alarms+=("eval: no metrics rollup for 36h — the eval_metrics worker is not running")
  fi
}

if [ "$EVAL_ONLY" = 1 ]; then
  { echo "mike eval audit — generated $(date -u +%FT%TZ)"; echo; eval_section; } > "$REPORT"
  ln -sfn "$REPORT" "$OUT_DIR/eval-latest.txt"
  find "$OUT_DIR" -maxdepth 1 -name 'eval-*.txt' -mtime +14 -delete
  alarms=()
  eval_alarms
  if [ ${#alarms[@]} -gt 0 ]; then
    { echo; echo "== ALARMS =="; printf '%s\n' "${alarms[@]}"; } >> "$REPORT"
    printf '%s\n' "${alarms[@]}" >&2
    shout "Claude eval audit: ${#alarms[@]} alarm(s)" "$(printf '%s\n' "${alarms[@]}")"
    exit 1
  fi
  echo "no eval alarms. report: $REPORT"
  exit 0
fi

{
  echo "mike usage audit — $SINCE to $STAMP (${DAYS}d)"
  echo "generated $(date -u +%FT%TZ)"
  echo

  echo "== totals =="
  q "WITH px AS (SELECT * FROM $PRICES)
     SELECT count(*) AS requests,
            round(sum(r.cache_read_input_tokens)/1e6) AS cache_read_m,
            round(sum(r.cache_creation_input_tokens)/1e6) AS cache_write_m,
            round(sum(r.output_tokens)/1e6,1) AS output_m,
            round(avg(r.cache_read_input_tokens)/1000) AS mean_ctx_k,
            round(sum($COST)/1e6)::int AS est_usd
     FROM requests.requests q
     JOIN responses.responses r ON r.request_id=q.id
     JOIN px ON px.model=q.model
     WHERE q.created_at >= '$SINCE';"
  echo

  echo "== spend by model seat =="
  q "WITH px AS (SELECT * FROM $PRICES)
     SELECT q.model, coalesce(q.subagent_persona,'(main thread)') AS seat,
            count(*) AS requests,
            round(avg(r.cache_read_input_tokens)/1000) AS mean_ctx_k,
            round(sum($COST)/1e6)::int AS est_usd
     FROM requests.requests q
     JOIN responses.responses r ON r.request_id=q.id
     JOIN px ON px.model=q.model
     WHERE q.created_at >= '$SINCE'
     GROUP BY 1,2 HAVING count(*) > 20
     ORDER BY 5 DESC;"
  echo

  echo "== top sessions =="
  q "WITH px AS (SELECT * FROM $PRICES)
     SELECT left(coalesce(s.title,'(untitled)'),42) AS session,
            count(*) AS requests,
            round(avg(r.cache_read_input_tokens)/1000) AS mean_ctx_k,
            round(sum($COST)/1e6)::int AS est_usd
     FROM requests.requests q
     JOIN responses.responses r ON r.request_id=q.id
     JOIN sessions.sessions s ON s.id=q.session_id
     JOIN px ON px.model=q.model
     WHERE q.created_at >= '$SINCE'
     GROUP BY 1 ORDER BY 4 DESC LIMIT 15;"
  echo

  echo "== cache re-write rate (guards temet-ai/mike#252) =="
  q "SELECT date_trunc('day', q.created_at)::date AS day,
            count(*) AS requests,
            count(*) FILTER (WHERE r.cache_creation_input_tokens > 150000) AS big_rewrites,
            round(100.0 * count(*) FILTER (WHERE r.cache_creation_input_tokens > 150000)
                  / nullif(count(*),0), 2) AS pct
     FROM requests.requests q
     JOIN responses.responses r ON r.request_id=q.id
     WHERE q.created_at >= '$SINCE'
     GROUP BY 1 HAVING count(*) > 100 ORDER BY 1;"
  echo

  echo "== dead steering rules (enabled, zero fires in window) =="
  q "SELECT r.id, left(r.value,64) AS anchor,
            coalesce(f.fires,0) AS fires
     FROM rules.rules r
     LEFT JOIN (
       SELECT fr.filter_rule_id AS rid, count(*) AS fires
       FROM requests.system_prompt_firing_rules fr
       JOIN requests.system_prompts rsp ON rsp.id = fr.request_system_prompt_id
       JOIN requests.requests q ON q.id = rsp.request_id
       WHERE q.created_at >= '$SINCE' GROUP BY 1
     ) f ON f.rid = r.id
     WHERE r.enabled AND coalesce(f.fires,0) = 0
       AND r.updated_at < now() - interval '24 hours'
     ORDER BY r.id;"
  echo

  echo "== projects with turns but no reachable lessons =="
  q "SELECT p.project_path, count(DISTINCT t.id) AS turns_in_window,
            (SELECT count(*) FROM memory.lessons l WHERE l.project_id = p.id) AS lessons
     FROM memory.operator_turns t
     JOIN projects.projects p ON p.id = t.project_id
     WHERE t.created_at >= '$SINCE'
     GROUP BY p.id, p.project_path
     HAVING (SELECT count(*) FROM memory.lessons l WHERE l.project_id = p.id) = 0
     ORDER BY 2 DESC;"
  echo

  echo "== turn attribution coverage =="
  q "SELECT (q.operator_turn_id IS NOT NULL) AS attributed,
            count(*) AS requests,
            round(sum(r.cache_read_input_tokens + coalesce(r.cache_creation_input_tokens,0))/1e6) AS tokens_m
     FROM requests.requests q
     JOIN responses.responses r ON r.request_id=q.id
     WHERE q.created_at >= '$SINCE' GROUP BY 1;"
  echo

  echo "== agent requests on the primary lane (#354) =="
  q "SELECT r.created_at::date AS day, count(*) AS requests
     FROM requests.requests r
     JOIN requests.turn_origins o ON o.id = r.turn_origin_id
     WHERE r.created_at >= '$SINCE' AND r.claude_code_agent_id IS NOT NULL
       AND o.code IN ('primary','primary_first')
     GROUP BY 1 ORDER BY 1;"
  echo

  echo "== routing guard refusals (main-thread loop limit, ~/.claude/hooks/main-loop-guard.sh) =="
  DENIALS="${AUDIT_DENIALS_LOG:-$HOME/.local/state/mike-routing/denials.log}"
  if [ -s "$DENIALS" ]; then
    jq -rs --arg since "$SINCE" '[.[] | select(.ts >= $since)] | if length == 0 then "no refusals in window" else (group_by(.model) | map({model: .[0].model, refusals: length, sessions: (map(.session_id) | unique | length), max_calls: (map(.calls) | max)}) | (["model","refusals","sessions","max_calls"] | @tsv), (.[] | [.model, .refusals, .sessions, .max_calls] | @tsv)) end' "$DENIALS" || echo "denials log unreadable: $DENIALS"
  else
    echo "no refusals logged"
  fi
  echo

  echo "== tool schema weight per request =="
  q "SELECT t.tool_name, max(d.byte_size)/4 AS approx_tokens,
            count(*) AS in_requests,
            round(sum(d.byte_size)/4.0/1e6,1) AS tokens_m_resent
     FROM requests.tools t
     JOIN tools.definitions d ON d.id = t.definition_id
     JOIN requests.requests q ON q.id = t.request_id
     WHERE q.created_at >= '$SINCE'
     GROUP BY 1 ORDER BY 4 DESC LIMIT 8;"
  echo

  eval_section
} > "$REPORT"

ln -sfn "$REPORT" "$OUT_DIR/latest.txt"

# ---- alarms -------------------------------------------------------------
alarms=()

worst_rewrite=$(qs "SELECT coalesce(max(pct),0) FROM (
    SELECT round(100.0 * count(*) FILTER (WHERE r.cache_creation_input_tokens > 150000)
                 / nullif(count(*),0), 2) AS pct
    FROM requests.requests q JOIN responses.responses r ON r.request_id=q.id
    WHERE q.created_at >= '$SINCE'
    GROUP BY date_trunc('day', q.created_at) HAVING count(*) > 100) z;")
if awk -v a="$worst_rewrite" -v b="$REWRITE_PCT_MAX" 'BEGIN{exit !(a+0 > b+0)}'; then
  alarms+=("cache re-write rate ${worst_rewrite}% exceeds ${REWRITE_PCT_MAX}% — check mike#252 has not regressed")
fi

mean_ctx=$(qs "SELECT coalesce(round(avg(r.cache_read_input_tokens)/1000),0)
    FROM requests.requests q JOIN responses.responses r ON r.request_id=q.id
    WHERE q.created_at >= '$SINCE';")
if awk -v a="$mean_ctx" -v b="$CONTEXT_MEAN_MAX" 'BEGIN{exit !(a+0 > b+0)}'; then
  alarms+=("mean context ${mean_ctx}k exceeds ${CONTEXT_MEAN_MAX}k per request — sessions are running too long")
fi

dead=$(qs "SELECT count(*) FROM rules.rules r
    WHERE r.enabled AND NOT EXISTS (
      SELECT 1 FROM requests.system_prompt_firing_rules fr
      JOIN requests.system_prompts rsp ON rsp.id = fr.request_system_prompt_id
      JOIN requests.requests q ON q.id = rsp.request_id
      WHERE fr.filter_rule_id = r.id AND q.created_at >= '$SINCE')
      AND r.updated_at < now() - interval '24 hours';")
if [ "${dead:-0}" -gt 0 ]; then
  alarms+=("$dead enabled steering rule(s) never fired — anchors likely stale")
fi

while IFS=$'\t' read -r lane_day lane_n; do
  [ -n "$lane_day" ] || continue
  alarms+=("lane: ${lane_n} primary-lane request(s) carrying an agent id on ${lane_day} — see mike#354")
done < <(psql "$PGURL" -P pager=off -At -F $'\t' --no-psqlrc -v ON_ERROR_STOP=1 -c "SELECT r.created_at::date, count(*)
    FROM requests.requests r
    JOIN requests.turn_origins o ON o.id = r.turn_origin_id
    WHERE r.created_at >= '$SINCE' AND r.claude_code_agent_id IS NOT NULL
      AND o.code IN ('primary','primary_first')
    GROUP BY 1 ORDER BY 1;")

eval_alarms

if [ ${#alarms[@]} -gt 0 ]; then
  { echo; echo "== ALARMS =="; printf '%s\n' "${alarms[@]}"; } >> "$REPORT"
  printf '%s\n' "${alarms[@]}" >&2
  echo "full report: $REPORT" >&2
  shout "Claude usage audit: ${#alarms[@]} alarm(s)" "$(printf '%s\n' "${alarms[@]}")"
  exit 1
fi

echo "no alarms. report: $REPORT"
