#!/usr/bin/env bash
# What a fleet run is doing, in one read-only call:
#   .claude/fleet/outcome-checks.sh <run id> ['YYYY-MM-DD HH:MM' UTC, to look only at what happened since then]
# The run id is in every seat's brief (fleet-<slug>-<8 hex>). Seats, start time, repository and
# branch are read from mike's own records of the run. Nothing is written.
set -u
RUN=${1:?run id, as shown in the fleet status}
INSPECT=${MIKE_INSPECT_ADDR:-127.0.0.1:9989}
DBURL=${MIKE_DATABASE_URL:-}
psqlr() {
  if [ -n "$DBURL" ] && command -v psql >/dev/null 2>&1; then
    PGOPTIONS='-c statement_timeout=30000 -c default_transaction_read_only=on' timeout 60 psql "$DBURL" -X -A -F ' | ' "$@"
  else
    PGOPTIONS='-c statement_timeout=30000 -c default_transaction_read_only=on' timeout 60 \
      docker exec -e PGOPTIONS armitage-pg psql -U devuser -d claude_proxy -X -A -F ' | ' "$@"
  fi
}
if ! DBPROBE=$(psqlr -t -c "select 1" 2>&1) || [ "$(printf '%s' "$DBPROBE" | tr -d ' \n')" != "1" ]; then
  echo "NOT DONE: this script cannot read the capture database from this machine ($(printf '%s' "$DBPROBE" | head -n 1))."
  echo "Set MIKE_DATABASE_URL to a reachable capture database with psql installed, or run it where the armitage-pg container runs."
  echo "Without a database, read the run through mike's tools instead: get_fleet, list_fleet_results, list_fleet_changes, list_session_tool_uses, get_workspace_roster."
  exit 2
fi
SINCE=${2:-$(psqlr -t -c "select to_char(min(created_at) at time zone 'UTC','YYYY-MM-DD HH24:MI') from launch.launches where run_id='$RUN'")}
[ -n "$SINCE" ] || { echo "no launches recorded for run $RUN in the database read (database: $([ -n "$DBURL" ] && echo MIKE_DATABASE_URL || echo container armitage-pg)); if seats are running, mike spawn could not write its ledger there, so read seat liveness with get_workspace_roster"; exit 1; }
FLEET=$(psqlr -t -c "select fleet_uuid from launch.fleets where run_id='$RUN'")
echo "run $RUN, fleet ${FLEET:-(no definition)}, since $SINCE UTC, read $(date -u '+%H:%M:%SZ')"
SEATS="select l.seat, case when l.seat like 'coordinator%' then 'coordinator' when l.seat like 'observer%' then 'observer'
    when l.seat like 'acceptance%' then 'acceptance' when l.seat like 'template-%' then 'template' when l.seat = 'reference' then 'reference' else 'worker' end kind,
    s.id sid, l.closed_at from launch.launches l join sessions.sessions s on s.session_uuid=l.session_uuid
  where l.run_id='$RUN' and l.seat is not null and l.seat <> 'status'"
MAIN="q.claude_code_agent_id is null and o.code in ('primary','primary_first')"
WRAPPERS=$(psqlr -t -c "select coalesce(string_agg(distinct regexp_replace(m[1],'[.,;:]+$',''),'|'),'') from launch.fleets f, jsonb_array_elements_text(f.definition->'rules') r, regexp_matches(r,'through ([A-Za-z0-9_./-]+)','g') m where f.run_id='$RUN' and r ~* '\m(cargo|build|test|make|gate|lint)\M'" 2>/dev/null | tr -d ' \n')
TOOLS="(scripts/cx|cargo|scripts/gate|scripts/lint-rust|make) "
CARGO="(^|&&|;|\\|\\||\\n|do |then )\\s*(cd [^;&]+(&&|;)\\s*)?(timeout [0-9]+ +)?([A-Z_]+=[^ ]+ +)*([^ ;&]*/)?$TOOLS"
[ -n "$WRAPPERS" ] && CARGO="($CARGO)|(($WRAPPERS) +(-[^ ]+ +)*([A-Z_]+=[^ ]+ +)*([^ ;&]*/)?$TOOLS)"
echo "database: $([ -n "$DBURL" ] && echo MIKE_DATABASE_URL || echo container armitage-pg); build wrappers from the definition: ${WRAPPERS:-none}"
NOTCARGO="git commit"

echo; echo "== 1 seats: requests, minutes active (gaps under 3 min), newest context, and the first request's cache (a fork that WROTE most of its context started cold)"
psqlr -c "with s as ($SEATS),
 r as (select s.seat, s.kind, s.closed_at, q.created_at, coalesce(rs.input_tokens,0)+coalesce(rs.cache_creation_input_tokens,0)+coalesce(rs.cache_read_input_tokens,0) ctx,
        coalesce(rs.cache_read_input_tokens,0) cr, coalesce(rs.cache_creation_input_tokens,0) cc,
        lag(q.created_at) over (partition by s.seat order by q.id) prev,
        row_number() over (partition by s.seat order by q.id) rn, row_number() over (partition by s.seat order by q.id desc) rnd
       from s join requests.requests q on q.session_id=s.sid and q.created_at > '$SINCE+00'
       join responses.responses rs on rs.request_id=q.id join requests.turn_origins o on o.id=q.turn_origin_id
       where $MAIN and rs.cache_read_input_tokens is not null)
 select seat, kind, count(*) reqs, to_char(max(created_at) at time zone 'UTC','HH24:MI') last_req,
   round(sum(case when prev is not null and created_at-prev < interval '3 minutes' then extract(epoch from created_at-prev) else 0 end)/60) active_min,
   max(ctx) filter (where rnd=1) context_now, max(cr) filter (where rn=1) first_read, max(cc) filter (where rn=1) first_written,
   case when max(cc) filter (where rn=1) > max(cr) filter (where rn=1) and kind='worker' then 'COLD START' else '' end flag,
   case when bool_and(closed_at is not null) then 'closed' else 'open' end seat_is
 from r group by seat, kind order by kind, seat"

echo; echo "== 2 tasks: each task's state and seat, and tasks that could start and have no seat"
if [ -n "$FLEET" ]; then
  TASKS=$(psqlr -t -c "select definition->'tasks' from launch.fleets where fleet_uuid='$FLEET'")
  curl -s --max-time 20 "http://$INSPECT/api/fleets/$FLEET/status" | python3 -c '
import json,sys
try: d=json.load(sys.stdin)
except Exception as e: print("NOT DONE: the status route did not answer:", e); sys.exit(0)
raw=json.loads(sys.argv[1]) if len(sys.argv)>1 and sys.argv[1] else {}
state={t["id"]:t["state"] for t in d.get("tasks",[])}
for t in d.get("tasks",[]):
    after=raw.get(t["id"],{}).get("after") or []
    free=all(state.get(a) in ("passed","done") for a in after)
    flag="  <-- CAN START, NO SEAT" if t["state"]=="queued" and not t.get("seat") and free else ""
    print(t["id"], "|", t["state"], "|", t.get("seat") or "-", "| after", ",".join(after) or "-", "| pass", t["passes"], "fail", t["fails"], flag)
' "$TASKS"
else echo "NOT DONE: no fleet definition holds this run id"; fi

echo; echo "== 3 waiting: gaps over 2 minutes on a seat's main thread, by what the seat did just before ('turn ended' is a seat waiting to be woken)"
psqlr -c "with s as ($SEATS),
 r as (select s.seat, s.kind, q.id, q.created_at, lead(q.created_at) over (partition by q.session_id order by q.created_at) nxt
       from s join requests.requests q on q.session_id=s.sid join requests.turn_origins o on o.id=q.turn_origin_id
       where q.created_at > '$SINCE+00' and $MAIN),
 gaps as (select r.*, extract(epoch from nxt-created_at)/60 mins,
       (select string_agg(u.tool_name||':'||left(coalesce(u.input->>'command',''),300),' || ') from tools.uses u where u.request_id=r.id) tools
       from r where nxt-created_at > interval '2 minutes')
 select seat, kind, case when tools is null then 'turn ended'
   when tools ~ '(scripts/cx|cargo |nextest|clippy|lint-rust|scripts/gate)' then 'build or test'
   when tools ~ '(sleep|task-wait|while |until |Monitor|ScheduleWakeup)' then 'waiting on a timer or a job'
   when tools ~ '(^|\|\| )Agent:' then 'own subagent' else 'other tool' end cause,
   count(*) gaps, round(sum(mins)) minutes, round(max(mins)) longest
 from gaps group by 1,2,3 order by 5 desc limit 30"

echo; echo "== 5 mail between kinds of seat: sent, reached the seat, acknowledged, delivered more than once (worker to worker near zero is seats working apart)"
psqlr -c "with s as ($SEATS)
 select f.kind from_kind, t.kind to_kind, count(*) msgs,
   count(*) filter (where m.channel_delivered_at is not null or m.sent_at is not null) reached,
   count(*) filter (where m.consumed_at is not null) acknowledged,
   count(*) filter (where m.delivery_attempts > 1) delivered_again
 from mailbox.messages m join s f on f.sid=m.from_session_id join s t on t.sid=m.to_session_id
 where m.queued_at > '$SINCE+00' group by 1,2 order by 3 desc"

echo; echo "== 6 notes per seat by kind (a worker with no result or finding notes is sharing nothing)"
psqlr -c "with s as ($SEATS) select s.seat, s.kind,
   count(*) filter (where n.kind='result') result, count(*) filter (where n.kind='finding') finding,
   count(*) filter (where n.kind='decision') decision, count(*) filter (where n.kind='task') task
 from s left join workspace.session_notes n on n.session_id=s.sid and n.created_at > '$SINCE+00' and n.kind <> 'brief'
 group by 1,2 order by 2,1"

echo; echo "== 7 build and test commands per seat, through a project's wrappers too (mike checks each pushed commit; no seat runs the full gate or lint-rust in the checkout)"
psqlr -c "with s as ($SEATS),
 b as (select s.seat, s.kind, u.input->>'command' c from s join tools.uses u on u.session_id=s.sid
       where u.created_at > '$SINCE+00' and u.tool_name='Bash' and u.input->>'command' ~ '$CARGO' and u.input->>'command' !~ '$NOTCARGO')
 select seat, kind, count(*) commands,
   count(*) filter (where c ~ '(cx check|cargo check)') checks, count(*) filter (where c ~ '(nextest|cargo test)') tests,
   count(*) filter (where c ~ 'clippy') clippy, count(*) filter (where c ~ '(scripts/gate|lint-rust( |$)|make (check|lint)([[:space:];&|]|$))') gate_or_lint,
   count(*) filter (where c ~ '(cx run|cargo run)') cargo_run, count(*) filter (where c ~ '(cx fmt|cargo fmt)') fmt_whole_crate,
   count(*) filter (where c ~ 'sqlx prepare') prepares,
   case when kind in ('coordinator','observer') then 'NOT A SEAT THAT BUILDS' else '' end flag
 from b group by 1,2 order by 3 desc"
echo "-- the same cargo command run by more than one seat (each repeat is a result another seat could have read)"
psqlr -c "with s as ($SEATS),
 b as (select s.seat, substring(u.input->>'command' from '((scripts/cx|cargo) [^;&|>]*)') c from s join tools.uses u on u.session_id=s.sid
       where u.created_at > '$SINCE+00' and u.tool_name='Bash' and u.input->>'command' ~ '$CARGO' and u.input->>'command' !~ '$NOTCARGO')
 select regexp_replace(trim(c), ' [0-9]$', '') command, count(*) runs, count(distinct seat) seats, string_agg(distinct seat, ' ') by_seats
 from b where c is not null group by 1 having count(distinct seat) > 1 order by 3 desc, 2 desc limit 15"

echo; echo "== 8 checkouts the run's cargo commands ran in (there is one checkout; any other line is a defect)"
psqlr -At -c "with s as ($SEATS) select distinct coalesce(substring(u.input->>'command' from 'cd (/[^ ;&]+)'), '(the seat''s own folder)')
   from s join tools.uses u on u.session_id=s.sid where u.created_at > '$SINCE+00' and u.tool_name='Bash'
   and u.input->>'command' ~ '$CARGO' and u.input->>'command' !~ '$NOTCARGO'" |
  while read -r d; do
    case "$d" in /*) root=$(git -C "$d" rev-parse --show-toplevel 2>/dev/null) || root="$d (not a checkout)";; *) root="$d";; esac
    echo "$root"; done | sort | uniq -c

echo; echo "== 8b changes in the checkout, read from the tree (rows in launch.tree_changes): per path, how many seats' commands were running when it changed"
psqlr -c "with s as ($SEATS) select c.path, count(*) changes, min(to_char(c.seen_at at time zone 'UTC','HH24:MI')) first, max(to_char(c.seen_at at time zone 'UTC','HH24:MI')) last,
   (select count(distinct s2.seat) from s s2 join tools.uses u on u.session_id=s2.sid join responses.responses rs on rs.request_id=u.request_id left join tools.results r on r.tool_use_id=u.id
      where u.tool_name in ('Bash','Edit','Write','MultiEdit','NotebookEdit') and rs.completed_at <= max(c.seen_at) + interval '1 second' and (r.created_at is null or r.created_at >= min(c.seen_at) - interval '1 second') and u.created_at > min(c.seen_at) - interval '30 minutes') seats_running
 from launch.tree_changes c where c.run_id='$RUN' and c.seen_at > '$SINCE+00' group by c.path order by changes desc limit 25"

echo; echo "== 8c the shared check (rows in launch.tree_checks): one run per state of the tree; askers beyond the first are runs that did not happen"
psqlr -c "select left(state,12) state, case when exit_code=0 then 'passed' else coalesce('failed '||exit_code, 'no exit') end verdict,
   extract(epoch from (finished_at-started_at))::int seconds, array_length(askers,1) askers, array_to_string(askers,', ') who, array_to_string(told,', ') told
 from launch.tree_checks where run_id='$RUN' order by id"
psqlr -c "select count(*) runs, coalesce(sum(array_length(askers,1)),0) asks, coalesce(sum(array_length(askers,1)-1),0) runs_saved,
   coalesce(sum(extract(epoch from (finished_at-started_at))*(array_length(askers,1)-1))::int,0) seconds_saved from launch.tree_checks where run_id='$RUN'"

echo; echo "== 8e the reference seat: questions mailed to it and answers it sent (a seat that is never asked has not earned its place)"
psqlr -c "with s as ($SEATS), ref as (select sid from s where kind='reference')
 select (select count(*) from mailbox.messages m, ref where m.to_session_id=ref.sid and m.queued_at > '$SINCE+00') asked,
        (select count(*) from mailbox.messages m, ref where m.from_session_id=ref.sid and m.queued_at > '$SINCE+00') answered,
        (select count(*) from s join tools.uses u on u.session_id=s.sid where s.kind='reference' and u.created_at > '$SINCE+00') tool_calls_by_reference,
        (select count(*) from s join tools.uses u on u.session_id=s.sid where s.kind='worker' and u.tool_name in ('Grep','Glob','Read') and u.created_at > '$SINCE+00') searches_and_reads_by_workers"

echo; echo "== 9 tokens by kind of seat, every lane"
psqlr -c "with s as ($SEATS) select s.kind, count(distinct s.seat) seats, count(*) reqs,
   round(sum(coalesce(r.cache_read_input_tokens,0))/1e6,1) read_millions, round(sum(coalesce(r.cache_creation_input_tokens,0))/1e6,2) written_millions,
   round(sum(coalesce(r.output_tokens,0))/1e3) output_thousands
 from s join requests.requests q on q.session_id=s.sid and q.created_at > '$SINCE+00' join responses.responses r on r.request_id=q.id
 group by 1 order by 4 desc"

echo; echo "== 10 memory and disk pressure on this machine"
free -h | sed -n '1,2p'; echo "io: $(sed -n 2p /proc/pressure/io 2>/dev/null)"

echo; echo "== 11 the change record and git reads, for this run's workers"
GITREAD='(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(--no-pager\s+)?(status|log|diff|show|rev-parse|ls-files|merge-base|fetch|branch|ls-tree|cat-file|rev-list|ls-remote|for-each-ref)\M'
GITWRITE='(^|[;&|(]\s*|\s)git\s+(-C\s+\S+\s+)?(add|commit|push|pull|checkout|reset|stash|merge|rebase|apply|rm|mv|cherry-pick|restore|switch|clone|worktree|update-index|update-ref|read-tree|commit-tree|tag|init|clean)\M'
WORKERS="with w as (select seat, sid from ($SEATS) x where kind='worker')"
STORED=$(psqlr -t -c "select count(*) from launch.run_records where run_id='$RUN'" 2>/dev/null | tr -d ' \n')
if [ "$STORED" = "1" ]; then
  echo "-- the run closed and mike stored its record (launch.run_records, GET /api/fleets/$FLEET/record): A and N below are read from it, not computed"
  echo "-- A reads of git state per 100 tool calls"
  psqlr -c "select worker_tool_calls tool_calls, worker_git_reads git_reads, git_reads_per_100_calls per_100_calls from launch.run_records where run_id='$RUN'"
else
echo "-- A reads of git state per 100 tool calls"
psqlr -c "$WORKERS,
 u as (select u.tool_name, u.input->>'command' cmd from w join tools.uses u on u.session_id=w.sid where u.created_at > '$SINCE+00')
 select count(*) tool_calls,
   count(*) filter (where tool_name='Bash' and cmd ~ '$GITREAD' and cmd !~ '$GITWRITE' and cmd !~ '(cargo|make |scripts/cx|scripts/gate|nextest|pnpm|npm |pytest|sb-test)') git_reads,
   round(100.0*count(*) filter (where tool_name='Bash' and cmd ~ '$GITREAD' and cmd !~ '$GITWRITE' and cmd !~ '(cargo|make |scripts/cx|scripts/gate|nextest|pnpm|npm |pytest|sb-test)')/nullif(count(*),0),1) per_100_calls from u"
fi
echo "-- D git status again within 2 minutes of the same seat's last one, no Edit or Write between"
psqlr -c "$WORKERS,
 st as (select u.session_id, u.created_at, lag(u.created_at) over (partition by u.session_id order by u.created_at) prev
        from w join tools.uses u on u.session_id=w.sid
        where u.created_at > '$SINCE+00' and u.tool_name='Bash' and u.input->>'command' ~ 'git\s+(-C\s+\S+\s+)?status')
 select count(*) status_calls, count(*) filter (where prev is not null and created_at-prev < interval '2 minutes') again_within_2_min,
   count(*) filter (where prev is not null and created_at-prev < interval '2 minutes' and not exists
     (select 1 from tools.uses e where e.session_id=st.session_id and e.tool_name in ('Edit','Write') and e.created_at > st.prev and e.created_at < st.created_at)) and_no_edit_between from st"
echo "-- N tool calls and minutes from each worker's first tool call to its first commit (a fleet_commit call or a git commit)"
if [ "$STORED" = "1" ]; then
  psqlr -c "select s->>'seat' seat, coalesce(s->>'calls_to_first_commit','no commit') calls_to_first_commit,
     s->>'minutes_to_first_commit' minutes_to_first_commit
   from launch.run_records r, jsonb_array_elements(r.seats) s where r.run_id='$RUN' and s->>'kind'='worker' order by 1"
else
psqlr -c "$WORKERS,
 u as (select w.seat, u.created_at, u.tool_name='mcp__mike__fleet_commit' or (u.tool_name='Bash' and u.input->>'command' ~ 'git\s+(-C\s+\S+\s+)?commit') is_commit
       from w join tools.uses u on u.session_id=w.sid where u.created_at > '$SINCE+00'),
 f as (select seat, min(created_at) filter (where is_commit) first_commit, min(created_at) first_call from u group by 1)
 select f.seat, case when f.first_commit is null then 'no commit' else (select count(*) from u where u.seat=f.seat and u.created_at <= f.first_commit)::text end calls_to_first_commit,
   round(extract(epoch from f.first_commit-f.first_call)/60,1) minutes_to_first_commit from f order by 1"
fi
echo "-- W share of this run's worker brief text that is lines shared by at least half of the worker briefs"
psqlr -c "with wb as (select l.id, l.brief_text from launch.launches l where l.run_id='$RUN' and l.seat !~ '^(template|coordinator|acceptance|observer|reference|status)' and l.brief_text is not null),
 ln as (select wb.id, line from wb, lateral regexp_split_to_table(wb.brief_text, E'\n') line where length(line) > 0),
 n as (select count(*) briefs from wb), f as (select line, count(distinct id) in_briefs from ln group by 1)
 select (select briefs from n) worker_briefs, sum(length(ln.line)) chars,
   round(100.0*sum(length(ln.line)) filter (where f.in_briefs*2 >= (select briefs from n))/nullif(sum(length(ln.line)),0)) pct_shared
 from ln join f using (line)"
echo "-- E file-change events (edit, shell_change) in this run's checkout since $SINCE: share naming a session"
psqlr -c "with c as (select distinct repo_root from launch.launches where run_id='$RUN' and repo_root is not null)
 select kind, count(*) events, count(*) filter (where session_id is not null) naming_a_session,
   round(100.0*count(*) filter (where session_id is not null)/nullif(count(*),0)) pct_naming
 from workspace.change_events e where e.checkout in (select repo_root from c) and e.at > '$SINCE+00' and e.kind in ('edit','shell_change')
 group by rollup(kind) order by 1 nulls last"
echo "-- M captured Edit and Write calls of the run's workers that have an edit event within 60 seconds on the same path with that seat as author"
psqlr -c "$WORKERS, c as (select distinct repo_root from launch.launches where run_id='$RUN' and repo_root is not null),
 t as (select w.seat, w.sid, u.created_at, u.file_path from w join tools.uses u on u.session_id=w.sid
       where u.created_at > '$SINCE+00' and u.tool_name in ('Edit','Write') and u.file_path is not null)
 select count(*) edit_write_calls,
   count(*) filter (where exists (select 1 from workspace.change_events e, c, unnest(e.paths) p
     where e.checkout=c.repo_root and e.at > '$SINCE+00' and e.kind='edit' and e.session_id=t.sid and c.repo_root || '/' || p = t.file_path
     and abs(extract(epoch from e.at - t.created_at)) < 60)) matched_with_that_seat_as_author,
   count(*) filter (where exists (select 1 from workspace.change_events e, c, unnest(e.paths) p
     where e.checkout=c.repo_root and e.at > '$SINCE+00' and e.session_id is distinct from t.sid and c.repo_root || '/' || p = t.file_path
     and abs(extract(epoch from e.at - t.created_at)) < 60)) event_names_another_or_none from t"

echo; echo "== 12 is the fleet slowing down: commits and working workers per hour since the start"
REPO=$(psqlr -t -c "select definition->>'repo' from launch.fleets where run_id='$RUN'" | tr -d ' ')
BRANCH=$(psqlr -t -c "select definition->>'branch' from launch.fleets where run_id='$RUN'" | tr -d ' ')
echo "-- commits per hour on $BRANCH (a fall to under half the first full hour while tasks are open is the fleet slowing)"
if [ -n "$REPO" ] && [ -n "$BRANCH" ] && git -C "$REPO" rev-parse --verify -q "$BRANCH" >/dev/null; then
  TZ=UTC git -C "$REPO" log "$BRANCH" --since="$SINCE UTC" --date=format-local:'%Y-%m-%d %H:00Z' --format='%cd' 2>/dev/null | sort | uniq -c
else echo "NOT DONE: repository ${REPO:-?} or branch ${BRANCH:-?} not readable here"; fi
echo "-- worker seats per hour: seats that made a request, and seats started (a start after the first hour is rework or a task added mid-run)"
psqlr -c "with s as ($SEATS), w as (select seat, sid from s where kind='worker'),
 h as (select date_trunc('hour', q.created_at) hr, count(distinct w.seat) working_workers
   from w join requests.requests q on q.session_id=w.sid and q.created_at > '$SINCE+00' group by 1)
 select to_char(h.hr at time zone 'UTC','HH24:00') as utc_hour, h.working_workers,
   (select count(*) from launch.launches l where l.run_id='$RUN' and l.seat is not null and l.seat !~ '^(coordinator|observer|acceptance|template-|reference|status)'
      and date_trunc('hour', l.created_at)=h.hr) workers_started
 from h order by h.hr"
echo "-- workers that made a request in the last 10 minutes, against worker seats still open"
psqlr -c "with s as ($SEATS) select count(distinct s.seat) filter (where s.closed_at is null) open_worker_seats,
   count(distinct s.seat) filter (where exists (select 1 from requests.requests q where q.session_id=s.sid and q.created_at > now() - interval '10 minutes')) working_now
 from s where s.kind='worker'"

echo; echo "== 13 what mike told the seats: each notice kind, how often, how many reached a seat's turn, how many died unread (a kind that never changes what a seat does next is a unit to cut)"
psqlr -c "with told as (select split_part(what,':',1) kind, count(*) marked from launch.told where run_id='$RUN' group by 1),
   mail as (select coalesce(substring(m.body from '^\[mike ([a-z -]+)\]'), 'other') kind,
       count(*) sent, count(*) filter (where m.consumed_at is not null or m.sent_at is not null) reached,
       count(*) filter (where m.dead_lettered_at is not null) dead, count(distinct m.to_session_id) seats
     from mailbox.messages m join sessions.sessions s on s.id=m.to_session_id join launch.launches l on l.session_uuid=s.session_uuid
    where l.run_id='$RUN' and m.from_session_id is null and m.body like '[mike %' and m.queued_at > '$SINCE+00' group by 1)
 select coalesce(mail.kind, told.kind) notice, told.marked ledger_keys, mail.sent, mail.reached, mail.dead, mail.seats
   from mail full outer join told on told.kind = mail.kind order by mail.sent desc nulls last, told.marked desc nulls last"
echo "-- judge rounds per task: reports filed, verdicts, fails (a task past three rounds opened a problem; see section 14)"
psqlr -c "select e.task, count(*) filter (where e.kind='report_filed') reports, count(distinct e.payload->>'note_uuid') filter (where e.kind='verdict_recorded') verdicts,
   count(distinct e.payload->>'note_uuid') filter (where e.kind='verdict_recorded' and (e.payload->>'passed')::boolean = false) failed_rounds
 from launch.fleet_events e where e.run_id='$RUN' and e.task is not null and e.kind in ('report_filed','verdict_recorded') group by 1 order by 2 desc, 1"

echo; echo "== 14 problems mike opened and what the solver decided (none opened means no trigger fired, not that nothing went wrong)"
psqlr -c "select to_char(e.occurred_at at time zone 'UTC','HH24:MI') at, e.kind, e.task, e.payload->>'trigger' trigger, left(coalesce(e.payload->>'decision', e.payload->>'detail', e.payload->>'question'), 140) what
 from launch.fleet_events e where e.run_id='$RUN' and e.kind like 'problem_%' order by e.occurred_at, e.id"
