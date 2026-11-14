INSERT INTO launch.run_records (
    fleet_uuid, run_id, project_id, measure_version, started_at, closed_at, seat_count, worker_count,
    working_minutes, waiting_minutes, first_commit_median_minutes, first_commit_longest_minutes,
    reports, reports_answered, report_to_verdict_median_minutes, report_to_verdict_longest_minutes,
    worker_tool_calls, worker_git_reads, git_reads_per_100_calls, cargo_commands,
    cargo_repeats_across_seats, questions_asked, questions_answered, forks, forks_waited,
    seats, mail_by_pair, tools_offered, tools_used,
    time_to_done_minutes, fault_minutes_total, tokens_per_accepted_task,
    manual_interventions_total, score)
SELECT $1, $2, mailbox.fleet_project_id($1), d.measure_version,
       coalesce((SELECT min(l.created_at) FROM launch.launches l WHERE l.run_id = $2), now()),
       coalesce((SELECT r.closed_at FROM launch.runs r WHERE r.run_id = $2), now()),
       d.seat_count, d.worker_count, d.working_minutes, d.waiting_minutes,
       d.first_commit_median_minutes, d.first_commit_longest_minutes,
       d.reports, d.reports_answered, d.report_to_verdict_median_minutes,
       d.report_to_verdict_longest_minutes, d.worker_tool_calls, d.worker_git_reads,
       d.git_reads_per_100_calls, d.cargo_commands, d.cargo_repeats_across_seats,
       d.questions_asked, d.questions_answered, d.forks, d.forks_waited,
       d.seats, d.mail_by_pair, d.tools_offered, d.tools_used,
       d.time_to_done_minutes, d.fault_minutes_total, d.tokens_per_accepted_task,
       d.manual_interventions_total, d.score
  FROM jsonb_to_record($3::jsonb) AS d(
       measure_version text, seat_count integer, worker_count integer,
       working_minutes double precision, waiting_minutes double precision,
       first_commit_median_minutes double precision, first_commit_longest_minutes double precision,
       reports bigint, reports_answered bigint,
       report_to_verdict_median_minutes double precision,
       report_to_verdict_longest_minutes double precision,
       worker_tool_calls bigint, worker_git_reads bigint, git_reads_per_100_calls double precision,
       cargo_commands bigint, cargo_repeats_across_seats bigint,
       questions_asked bigint, questions_answered bigint, forks bigint, forks_waited bigint,
       seats jsonb, mail_by_pair jsonb, tools_offered jsonb, tools_used jsonb,
       time_to_done_minutes double precision, fault_minutes_total double precision,
       tokens_per_accepted_task double precision, manual_interventions_total bigint,
       score jsonb)
ON CONFLICT (fleet_uuid) DO NOTHING
RETURNING id
