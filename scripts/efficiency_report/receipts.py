"""Join durable receipts to this host's optional monotonic timing observations."""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess

from .collect import query


def timing_total(receipts, directory):
    queued, execution, covered = 0, 0, 0
    for receipt in receipts:
        if not isinstance(receipt.get('id'), int) or receipt.get('finished_at') is None:
            continue
        try:
            value = json.loads((directory / (str(receipt['id'])+'.json')).read_text())
            if value.get('schema') != 'operation-timing-v1' or value.get('receipt') != receipt['id'] or value.get('ledger') != receipt['run_id']:
                continue
            q, e = value['queue_and_setup_ms'], value['execution_and_cleanup_ms']
            if not all(type(n) is int and n >= 0 for n in (q, e)):
                continue
            queued += q
            execution += e
            covered += 1
        except (OSError, ValueError, KeyError):
            continue
    return dict(managed_receipts=len(receipts), receipts_with_timing=covered,
                observed_queue_and_setup_seconds=queued/1000 if covered else None,
                observed_execution_and_cleanup_seconds=execution/1000 if covered else None,
                timing_complete=bool(receipts) and covered == len(receipts))


def measurements(status, sessions):
    until = status.get('finished_at') or datetime.now(timezone.utc).isoformat()
    try:
        receipts = query('receipts', sessions, status['started_at'], until, status['run_id'])
        goals = query('goals', sessions, status['started_at'], until)[0]
        state = Path(os.environ.get('MIKE_STATE_DIR', str(Path(os.environ.get('XDG_STATE_HOME', str(Path.home()/'.local/state'))) / 'mike')))
        measured = timing_total(receipts, state/'managed-builds/timings')
        return dict(managed_operations=measured,
                    observed_goal_limit_stops=goals['observed_goal_limit_stops'],
                    goal_stop_coverage='Events recorded by the updated mod; historical and failed telemetry are unknown')
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired):
        return dict(managed_operations=None, observed_goal_limit_stops=None,
                    goal_stop_coverage='receipt or event query unavailable')
