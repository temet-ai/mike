# Push-wake query plans

Measured on `rc_scratch_b1`, with 10,000 synthetic sessions, channels, mailbox messages, injection records and stream calls. The live database has neither new migration and was not modified. These are local plan baselines, not live-load evidence. Fixture: `artifacts/2026-09-08/remote-control-push-wake/perf-fixture.sql`. Each EXPLAIN executes the mutation inside BEGIN/ROLLBACK; no planner settings were overridden. Three runs per query, minimum timing below.


## remote_control_reserve_injection

Runs: [1.639, 2.841, 1.856] ms; new query baseline.

    Perf: remote_control_reserve_injection (hot tier) — 1.639 ms, scratch fixture, 5 ms budget.
    Perf-Query-SHA: e32793a4daddf5a2
    Perf-Verdict: baseline


Scans: Index Scan using idx_rc_channels_local_session_uuid on channels c  (cost=0.29..8.30 rows=1 width=30) (actual time=0.013..0.014 rows=1 loops=1), Index Scan using messages_message_uuid_key on messages m  (cost=0.29..8.30 rows=1 width=14) (actual time=0.026..0.028 rows=1 loops=1), Index Scan using sessions_session_uuid_key on sessions s  (cost=0.29..8.30 rows=1 width=30) (actual time=0.011..0.011 rows=1 loops=1)


## remote_control_finish_injection

Runs: [1.74, 1.487, 1.529] ms; new query baseline.

    Perf: remote_control_finish_injection (hot tier) — 1.487 ms, scratch fixture, 5 ms budget.
    Perf-Query-SHA: cf8a1889243830c9
    Perf-Verdict: baseline


Scans: Index Scan using injections_pkey on injections  (cost=0.29..8.30 rows=1 width=22) (actual time=0.025..0.027 rows=1 loops=1), Index Scan using messages_message_uuid_key on messages m  (cost=0.29..8.30 rows=1 width=42) (actual time=0.014..0.014 rows=1 loops=1)


## remote_control_ack_injections

Runs: [6.074, 1.753, 2.112] ms; new query baseline.

    Perf: remote_control_ack_injections (hot tier) — 1.753 ms, scratch fixture, 5 ms budget.
    Perf-Query-SHA: 8bc658ff6af6c04c
    Perf-Verdict: baseline


Scans: Index Scan using channels_cse_id_key on channels c  (cost=0.29..8.30 rows=1 width=14) (actual time=0.023..0.023 rows=1 loops=1), Index Scan using injections_pkey on injections i  (cost=0.29..8.30 rows=1 width=14) (actual time=0.033..0.035 rows=1 loops=1), Index Scan using messages_message_uuid_key on messages m  (cost=0.29..8.30 rows=1 width=42) (actual time=0.008..0.008 rows=1 loops=1)


## remote_control_close_stream

Runs: [0.499, 0.407, 0.466] ms; before 0.428 ms.

    Perf: remote_control_close_stream (hot tier) — 0.407 ms, scratch fixture, 5 ms budget.
    Perf-Query-SHA: b20ae70ddb02afed
    Perf-Verdict: baseline


Scans: Index Scan using calls_pkey on calls  (cost=0.29..8.30 rows=1 width=14) (actual time=0.086..0.088 rows=1 loops=1), Index Scan using channels_pkey on channels ch  (cost=0.29..8.30 rows=1 width=38) (actual time=0.003..0.003 rows=1 loops=1)
