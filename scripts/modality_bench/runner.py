"""Run one cell: prepare its directory, launch the arm, cap it, record what happened, grade it
and read it from capture. The fleet arm goes through the inspect API as scripts/fleet-ref does."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import urllib.error
import urllib.request

from . import arms, capture, grade, preflight
from .ask import render_ask

POLL_SECONDS = 20


def now():
    return time.time()


def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, default=str) + '\n')


def http_json(url, method='GET', body=None, timeout=60):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method, headers={'content-type': 'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            text = response.read().decode()
            return response.status, (json.loads(text) if text.strip() else {})
    except urllib.error.HTTPError as err:
        text = err.read().decode(errors='replace')
        try:
            return err.code, json.loads(text)
        except ValueError:
            return err.code, {'error': text[:500]}


class Hosts:
    def __init__(self, root, inspect_url, proxy_url):
        self.root, self.inspect_url, self.proxy_url = Path(root), inspect_url.rstrip('/'), proxy_url

    def settings(self):
        status, payload = http_json(f'{self.inspect_url}/api/settings')
        if status != 200:
            raise SystemExit(f'GET /api/settings answered {status}')
        return preflight.settings_map(payload)

    def filter_rules(self):
        status, payload = http_json(f'{self.inspect_url}/api/filter/rules')
        return preflight.filter_rule_count(payload) if status == 200 else 0

    def fleets(self):
        status, payload = http_json(f'{self.inspect_url}/api/fleets')
        return payload if status == 200 else []

    def refusals(self, plan):
        settings = self.settings()
        refusals = preflight.settings_refusals(settings, self.filter_rules())
        refusals += preflight.host_refusals(self.root / '.deploy' / 'deploy.lock', self.fleets())
        live_hash = preflight.settings_hash(settings)
        pinned = plan['environment'].get('settings_hash')
        if pinned and live_hash != pinned:
            refusals.append('mike settings changed since the plan was frozen')
        version = preflight.client_version()
        if version != plan['environment'].get('client_version'):
            refusals.append(f'claude is {version}, the plan pinned {plan["environment"].get("client_version")}')
        return refusals


def prepare_cell(root, cell_dir, install=True):
    """scripts/fleet-ref prepare node --dir <cell>: work, origin.git and the hidden answers."""
    env = dict(os.environ)
    if not install:
        env['FLEET_REF_NO_INSTALL'] = '1'
    done = subprocess.run([str(Path(root) / 'scripts' / 'fleet-ref'), 'prepare', 'node', '--dir', str(cell_dir)],
                          capture_output=True, text=True, env=env, check=False, timeout=900)
    if done.returncode != 0:
        raise SystemExit(f'fleet-ref prepare failed for {cell_dir}: {done.stderr.strip()[-800:]}')
    return dict(line.split('=', 1) for line in done.stdout.splitlines() if '=' in line)


def run_claude(argv, env, cwd, cap_seconds):
    started = now()
    process = subprocess.Popen(argv, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=cap_seconds)
        stop = 'done' if process.returncode == 0 else 'error'
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        stop = 'timeout'
    result = {'exit': process.returncode, 'stop': stop, 'wall_seconds': round(now() - started, 1),
              'stderr_tail': (stderr or '')[-2000:]}
    try:
        parsed = json.loads(stdout) if stdout.strip() else None
    except ValueError:
        parsed = {'raw': stdout[-4000:]}
    if isinstance(parsed, dict) and parsed.get('is_error') and stop == 'done':
        result['stop'] = 'error'
    if isinstance(parsed, dict) and 'budget' in str(parsed.get('result', '')).lower() and 'exceed' in str(parsed.get('result', '')).lower():
        result['stop'] = 'budget'
    return result, parsed


def fleet_definition(spec, plan, work, origin):
    text = json.dumps(spec['definition'])
    for key, value in (('__REPO__', work), ('__ORIGIN__', origin), ('__BRANCH__', 'main')):
        text = text.replace(key, value)
    definition = json.loads(text)
    for worker in definition.get('workers', {}).values():
        worker['model'] = plan['model']
    return definition


def run_fleet(hosts, spec, plan, cell, work, origin, cap_seconds):
    """Define, ready, start, poll to done or the cap; stop at the cap. Mirrors scripts/fleet-ref run."""
    slug = f"mbench-{cell['id']}-{time.strftime('%Y%m%d%H%M%S')}"
    definition = fleet_definition(spec, plan, work, origin)
    status, reply = http_json(f'{hosts.inspect_url}/api/fleets', 'POST',
                              {'slug': slug, 'project_path': work, 'definition': definition})
    if status // 100 != 2:
        return {'stop': 'error', 'exit': 1, 'wall_seconds': 0, 'slug': slug, 'error': f'define refused ({status}): {reply}'}, reply
    missing = reply.get('missing') or []
    if missing:
        return {'stop': 'error', 'exit': 1, 'wall_seconds': 0, 'slug': slug, 'error': f'definition missing: {missing}'}, reply
    started = now()
    status, reply = http_json(f'{hosts.inspect_url}/api/fleets/{slug}', 'PATCH', {'ready': True})
    if status // 100 != 2:
        return {'stop': 'error', 'exit': 1, 'wall_seconds': 0, 'slug': slug, 'error': f'ready refused ({status}): {reply}'}, reply
    status, reply = http_json(f'{hosts.inspect_url}/api/fleets/{slug}/start', 'POST', {})
    if status // 100 != 2 and status != 409:
        return {'stop': 'error', 'exit': 1, 'wall_seconds': 0, 'slug': slug, 'error': f'start refused ({status}): {reply}'}, reply
    state, last = '', {}
    while True:
        status, last = http_json(f'{hosts.inspect_url}/api/fleets/{slug}/status', timeout=30)
        state = last.get('state') or (last.get('fleet') or {}).get('state') or ''
        if state in ('done', 'failed'):
            break
        if now() - started > cap_seconds:
            http_json(f'{hosts.inspect_url}/api/fleets/{slug}/stop', 'POST', {})
            state = 'timeout'
            break
        time.sleep(POLL_SECONDS)
    record_status, record = http_json(f'{hosts.inspect_url}/api/fleets/{slug}/record')
    result = {'stop': {'done': 'done', 'failed': 'error', 'timeout': 'timeout'}[state], 'exit': 0 if state == 'done' else 1,
              'wall_seconds': round(now() - started, 1), 'slug': slug,
              'record': record if record_status == 200 else None}
    return result, last


def run_cell(root, hosts, spec, plan, cell, plan_dir, agent=None, read_capture=True, install=True):
    """One cell, start to finish. `agent` is a callable for a dry run (scripted answers, no model)."""
    cell_dir = Path(plan_dir) / 'cells' / cell['id']
    cell_dir.mkdir(parents=True, exist_ok=True)
    record_path = cell_dir / 'cell.json'
    prepared = prepare_cell(root, cell_dir, install=install)
    work, origin = prepared['work'], prepared['origin']
    record = {'id': cell['id'], 'arm': cell['arm'], 'rep': cell['rep'], 'session': cell['session'],
              'project_path': work, 'prepared': prepared, 'started_at': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'host_before': preflight.host_snapshot()}
    arm = cell['arm']
    if arm != 'fleet':
        ask = render_ask(spec, arm)
        (cell_dir / 'ask.md').write_text(ask)
        record['home'] = arms.seed_home(cell_dir, global_config=arms.TEAM_GLOBAL_CONFIG if arm == 'team' else None)
        record['argv'] = arms.claude_argv(arm, plan, cell, '<ask.md>', plan.get('budget_usd'))
    if agent is not None:
        result, output = agent(spec, plan, cell, cell_dir)
    elif arm == 'fleet':
        result, output = run_fleet(hosts, spec, plan, cell, work, origin, plan['cap_seconds'])
    else:
        argv = arms.claude_argv(arm, plan, cell, ask, plan.get('budget_usd'))
        env = arms.claude_env(arm, plan, cell_dir, hosts.proxy_url)
        result, output = run_claude(argv, env, work, plan['cap_seconds'])
    write_json(cell_dir / 'stdout.json', output)
    record.update(result)
    record['host_after'] = preflight.host_snapshot()
    record['grade'] = grade.grade_cell(spec, cell_dir)
    if read_capture:
        try:
            snapshot = capture.read_cell(capture.database_url(root), work)
            record['capture'] = snapshot
            record['totals'] = capture.totals(snapshot)
            ok, reasons = capture.fidelity(arm, snapshot)
            record['fidelity'] = {'ok': ok, 'reasons': reasons}
        except SystemExit as err:
            record['capture_error'] = str(err)
            record['fidelity'] = {'ok': None, 'reasons': ['capture unreadable']}
    else:
        record['fidelity'] = {'ok': None, 'reasons': ['capture not read']}
    write_json(record_path, record)
    return record


def scripted_agent(wrong=(), push=True):
    def agent(spec, _plan, _cell, cell_dir):
        started = now()
        pushed = grade.scripted_solve(spec, cell_dir, wrong=wrong, push=push)
        return {'exit': 0 if pushed else 1, 'stop': 'done' if pushed else 'error',
                'wall_seconds': round(now() - started, 1), 'scripted': True}, {'scripted': True, 'wrong': list(wrong)}
    return agent
