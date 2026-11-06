"""Read a cell from mike's capture by its project path, and say whether the arm's modality
actually fired. Read-only SQL through psql; the same join the compressor trials use."""
import json
import os
from pathlib import Path
import shutil
import subprocess

from .plan import WEIGHTS

CELL_SQL = """
SELECT row_to_json(a) FROM (
  WITH s AS (SELECT s.id, s.session_uuid FROM sessions.sessions s
             JOIN projects.projects p ON p.id = s.project_id WHERE p.project_path = '{path}')
  SELECT
    (SELECT count(*) FROM s) AS sessions,
    (SELECT coalesce(json_agg(row_to_json(t)), '[]') FROM (
       SELECT o.code AS lane, count(*) AS requests,
              count(*) FILTER (WHERE q.claude_code_agent_id IS NOT NULL) AS agent_id_requests,
              sum(coalesce(r.input_tokens, 0)) AS fresh,
              sum(coalesce(r.cache_creation_input_tokens, 0)) AS cache_write,
              sum(coalesce(r.cache_read_input_tokens, 0)) AS cache_read,
              sum(coalesce(r.output_tokens, 0)) AS output
         FROM s JOIN requests.requests q ON q.session_id = s.id
         JOIN requests.turn_origins o ON o.id = q.turn_origin_id
         LEFT JOIN responses.responses r ON r.request_id = q.id
        GROUP BY o.code) t) AS lanes,
    (SELECT coalesce(json_agg(row_to_json(t)), '[]') FROM (
       SELECT u.tool_name, count(*) AS uses FROM s JOIN tools.uses u ON u.session_id = s.id
        GROUP BY u.tool_name) t) AS tools,
    (SELECT count(*) FROM launch.launches l JOIN s ON s.session_uuid = l.session_uuid
      WHERE l.run_id IS NOT NULL AND l.seat IS NOT NULL) AS seat_launches
) a;
"""


def database_url(root):
    if os.environ.get('MIKE_DATABASE_URL'):
        return os.environ['MIKE_DATABASE_URL']
    env = Path(root) / '.env'
    if env.is_file():
        for line in env.read_text().splitlines():
            if line.startswith('MIKE_DATABASE_URL='):
                return line.split('=', 1)[1].strip().strip('"\'')
    raise SystemExit('MIKE_DATABASE_URL is not set and no .env beside the script names it')


def run_sql(url, sql):
    env = dict(os.environ, PGOPTIONS='-c default_transaction_read_only=on -c statement_timeout=60000')
    done = subprocess.run([shutil.which('psql') or 'psql', url, '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-c', sql],
                          env=env, capture_output=True, text=True, check=False, timeout=90)
    if done.returncode != 0:
        raise SystemExit(f'capture query failed: {done.stderr.strip()}')
    return done.stdout


def read_cell(url, project_path):
    if "'" in project_path:
        raise ValueError('a project path with a quote is not queried')
    out = run_sql(url, CELL_SQL.format(path=project_path)).strip()
    return json.loads(out) if out else {'sessions': 0, 'lanes': [], 'tools': [], 'seat_launches': 0}


def totals(capture):
    """Tokens summed over every lane, plus the weighted relative cost."""
    sums = {key: 0 for key in ('requests', 'fresh', 'cache_write', 'cache_read', 'output')}
    for lane in capture.get('lanes', []):
        for key in sums:
            sums[key] += int(lane.get(key) or 0)
    sums['weighted'] = round(sum(sums[k] * WEIGHTS[k] for k in WEIGHTS))
    sums['tool_calls'] = sum(int(t.get('uses') or 0) for t in capture.get('tools', []))
    sums['sessions'] = int(capture.get('sessions') or 0)
    return sums


def lane(capture, code):
    return next((l for l in capture.get('lanes', []) if l.get('lane') == code), None)


def uses(capture, tool):
    return sum(int(t.get('uses') or 0) for t in capture.get('tools', []) if t.get('tool_name') == tool)


def fidelity(arm, capture):
    """Did the arm's modality fire? (ok, reasons) from the capture alone."""
    reasons = []
    sub = lane(capture, 'subagent')
    wf = lane(capture, 'workflow_subagent')
    sessions = int(capture.get('sessions') or 0)
    if arm == 'single':
        if sub or wf:
            reasons.append('requests on a subagent lane')
        if sessions > 1:
            reasons.append(f'{sessions} sessions at the project path')
        for tool in ('Agent', 'TeamCreate', 'Workflow'):
            if uses(capture, tool):
                reasons.append(f'{tool} was used')
    elif arm == 'subagents':
        if not sub or not uses(capture, 'Agent'):
            reasons.append('no subagent requests or no Agent use')
        if wf:
            reasons.append('workflow subagent requests')
    elif arm == 'team':
        if not uses(capture, 'TeamCreate'):
            reasons.append('TeamCreate was not used')
        if sessions < 2 and not uses(capture, 'SendMessage'):
            reasons.append('one session and no teammate messages')
    elif arm == 'workflow':
        if not uses(capture, 'Workflow'):
            reasons.append('Workflow was not used')
        if not wf:
            reasons.append('no workflow subagent requests')
    elif arm == 'fleet':
        if int(capture.get('seat_launches') or 0) == 0:
            reasons.append('no seat launches at the project path')
    if sessions == 0:
        reasons.append('nothing captured at the project path')
    return not reasons, reasons
