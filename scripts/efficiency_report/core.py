"""Explicit denominators and missing-data handling for fleet efficiency reports."""
from datetime import datetime


def stamp(value):
    parsed = datetime.fromisoformat(value.replace('Z', '+00:00'))
    if parsed.tzinfo is None:
        raise ValueError('timestamps require a timezone')
    return parsed.timestamp()


def accepted_tasks(status):
    accepted = []
    for task in status.get('tasks', []):
        if task.get('state') != 'passed' or not task.get('criteria'):
            continue
        eligible = True
        for obligation in status.get('obligations', []):
            if not obligation.get('required'):
                continue
            tasks, criteria = obligation.get('tasks', []), obligation.get('criteria', [])
            if tasks and task['id'] not in tasks:
                continue
            if criteria and not set(criteria).intersection(task['criteria']):
                continue
            passed = obligation.get('outcome') == 'accepted' and obligation.get('issue') is None
            if obligation.get('data', {}).get('ruling') is not None:
                passed = passed and any(p.get('task') == task['id'] and p.get('outcome') == 'passed'
                                        and p.get('check_id') is not None and p.get('problem') is None
                                        for p in obligation.get('ruling_proof', []))
            eligible = eligible and passed
        if eligible:
            accepted.append(task['id'])
    return accepted


def summarize(project, status, record, usage, session_count):
    accepted = accepted_tasks(status)
    gaps = []
    closed = status.get('state') == 'done' and bool(status.get('finished_at'))
    if not closed:
        gaps.append('open_run_right_censored')
    started, finished = status.get('started_at'), status.get('finished_at')
    elapsed = max(0, stamp(finished)-stamp(started)) if closed and started else None
    if not record:
        gaps.append('missing_run_record')
    if record and record.get('run_id') != status.get('run_id'):
        gaps.append('run_record_identity_mismatch')
        record = {}
    complete = bool(usage and usage.get('requests', 0) > 0
                    and usage.get('requests') == usage.get('complete_requests')
                    and usage.get('resolved_sessions') == session_count and session_count > 0)
    if not complete:
        gaps.append('missing_or_incomplete_usage')
    v2 = record.get('measure_version') == 'run-record-v2-declared-operations'
    if not v2:
        gaps.append('legacy_operation_measure_not_comparable')
    denominator = len(accepted) if closed and accepted else None
    return dict(project=project, fleet=status.get('fleet'), run_id=status.get('run_id'),
                state=status.get('state'), accepted_task_ids=accepted, accepted_tasks=len(accepted),
                task_count=len(status.get('tasks', [])), elapsed_seconds=elapsed,
                elapsed_seconds_per_accepted_task=elapsed/denominator if elapsed is not None and denominator else None,
                observed_tokens=usage.get('observed_tokens') if usage else None,
                observed_tokens_per_accepted_task=usage['observed_tokens']/denominator if usage and usage.get('observed_tokens') is not None and denominator else None,
                tokens_per_accepted_task=usage['observed_tokens']/denominator if complete and denominator else None,
                usage_complete=complete, usage_coverage=usage,
                operation_repeats=record.get('operation_repeats_across_seats') if v2 else None,
                direct_operation_bypasses=record.get('direct_operation_bypasses') if v2 else None,
                working_agent_minutes=record.get('working_minutes'),
                waiting_agent_minutes=record.get('waiting_minutes'),
                gaps=gaps)
