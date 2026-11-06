"""Read existing inspect endpoints and bounded, read-only usage aggregates."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import urllib.parse
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]


def get(base, path, query=None):
    if query:
        path += '?' + urllib.parse.urlencode(query)
    with urllib.request.urlopen(base.rstrip('/') + path, timeout=30) as response:
        return json.load(response)


def query(name, sessions, since, until, run=""):
    if name not in ("usage", "receipts", "goals"):
        raise ValueError("unknown query")
    sessions = sorted({str(uuid.UUID(s)) for s in sessions})
    options = '-c default_transaction_read_only=on -c statement_timeout=5000 -c lock_timeout=1000'
    env = dict(os.environ, PGOPTIONS=options)
    url = env.get('MIKE_DATABASE_URL')
    if not url and (ROOT / '.env').exists():
        for line in (ROOT / '.env').read_text().splitlines():
            if line.startswith('MIKE_DATABASE_URL='):
                url = line.split('=', 1)[1].strip().strip('"').strip("'")
    if url and shutil.which('psql'):
        command = ['psql', url]
    else:
        command = ['docker', 'exec', '-i', '-e', 'PGOPTIONS='+options, 'armitage-pg',
                   'psql', '-U', 'devuser', '-d', 'claude_proxy']
    command += ['-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-v', 'sessions='+json.dumps(sessions),
                '-v', 'since='+since, '-v', 'until='+until, '-v', 'run='+run]
    query = (Path(__file__).parent / (name+'.sql')).read_text()
    result = subprocess.run(command, input='SELECT row_to_json(measure) FROM ('+query+') measure;',
                            text=True, capture_output=True, timeout=15, env=env)
    if result.returncode:
        raise RuntimeError('Read-only usage query failed; no zero-value fallback was used')
    return [json.loads(line) for line in result.stdout.splitlines() if line]


def usage(sessions, since, until):
    return query('usage', sessions, since, until)[0]


def collect(base, projects, limit):
    from .core import summarize
    from datetime import datetime, timezone
    rows, errors = [], []
    for project in projects:
        fleets = get(base, '/api/fleets', dict(project_path=project, limit=limit))['fleets']
        for fleet in fleets:
            if not fleet.get('run_id'):
                continue
            key = str(uuid.UUID(fleet['fleet']))
            try:
                status = get(base, f'/api/fleets/{key}/status')
                try:
                    record = get(base, f'/api/fleets/{key}/record')
                except (OSError, ValueError):
                    record = {}
                sessions = {s['session'] for s in status.get('seats', []) if s.get('session')}
                measured = None
                if sessions and status.get('started_at'):
                    try:
                        measured = usage(sessions, status['started_at'],
                                         status.get('finished_at') or datetime.now(timezone.utc).isoformat())
                    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired):
                        errors.append(dict(fleet=key, measurement='usage_unavailable'))
                row = summarize(project, status, record, measured, len(sessions))
                full = get(base, f'/api/fleets/{key}')
                row['operation_adoption'] = full.get('operation_adoption')
                if status.get('started_at'):
                    from .receipts import measurements
                    row.update(measurements(status, sessions))
                row['stack_disciplines'] = sorted({t.get('discipline', 'unknown') for t in status.get('tasks', [])})
                rows.append(row)
            except (OSError, ValueError, KeyError):
                errors.append(dict(fleet=key, measurement='fleet_unavailable'))
    return dict(schema='fleet-efficiency-v1', generated_at=datetime.now(timezone.utc).isoformat(),
                scope='Newest runs per explicit project; roster sessions within each run; no cross-project pooling or causal savings claim',
                denominator='One task with passed criteria and accepted required obligations; whole-run cost divided by accepted task count',
                token_unit='All reported input, output, cache-read and cache-write tokens; this is not dollar cost',
                coverage='Spawned sessions outside the roster are excluded; delegated work sharing a roster session is included',
                limit_per_project=limit, runs=rows, errors=errors)
