"""Plan, run and collect paired full-task compressor trials. Arms come from chosen session ids,
so no shared setting changes; each session reads, idles past its prompt cache, then resumes."""
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import threading
import time
import uuid

from .qualification import policy_hash, qualifies
from .trial_tasks import BY_ID, TASKS, WEAKENED, build_tree, first_prompt, grade, ground_truth_hash, task_hash

SCHEMA = 'compressor-qualification-v1'
COLD_WAIT_SECONDS = 3630
WEIGHTS = {'fresh': 1.0, 'cache_write': 1.25, 'cache_read': 0.1, 'output': 5.0}
CAPTURE_SQL = """
SELECT row_to_json(a) FROM (SELECT
  (SELECT coalesce(json_agg(row_to_json(t) ORDER BY t.id), '[]') FROM (
     SELECT q.id, q.model, r.input_tokens AS fresh, r.cache_creation_input_tokens AS cache_write,
            r.cache_read_input_tokens AS cache_read, r.output_tokens AS output
       FROM requests.requests q JOIN responses.responses r ON r.request_id = q.id
      WHERE q.session_id = s.id) t) AS turns,
  (SELECT coalesce(json_agg(row_to_json(t) ORDER BY t.id), '[]') FROM (
     SELECT u.id, u.request_id, u.tool_name, u.file_path, u.input, u.created_at, x.content, x.is_error
       FROM tools.uses u LEFT JOIN tools.results x ON x.tool_use_id = u.id
      WHERE u.session_id = s.id) t) AS calls,
  (SELECT coalesce(json_agg(row_to_json(t) ORDER BY t.id), '[]') FROM (
     SELECT q.id, st->>'outcome' AS outcome, st->>'reason_code' AS reason, st->'resolved_parameters'->>'arm' AS arm,
            st->>'input_sha256' AS original_sha256, st->>'output_sha256' AS sent_sha256
       FROM requests.requests q, jsonb_array_elements(q.transformation_manifest->'stages') st
      WHERE q.session_id = s.id AND st->>'stage' = 'file_read_compression') t) AS stages
  FROM sessions.sessions s WHERE s.session_uuid = '{session}') a;
"""


def now_iso():
    return datetime.now(timezone.utc).isoformat()


def bucket(session_id):
    return int(hashlib.sha256(session_id.encode()).hexdigest()[:8], 16) % 100


def session_for(arm, share, rng):
    while True:
        candidate = str(uuid.UUID(int=rng.getrandbits(128), version=4))
        place = bucket(candidate)
        if (arm == 'compact' and place < share) or (arm == 'original' and place >= max(share, 95)):
            return candidate


def run_entry(task, rep, arm, share, rng, config_hash, label='pair'):
    return {'task_id': task.id, 'family': task.family, 'rep': rep, 'arm': arm, 'kind': label,
            'session': session_for(arm, share, rng), 'task_hash': task_hash(task),
            'ground_truth_hash': ground_truth_hash(task), 'model_config_hash': config_hash}


def make_plan(model, effort, client_version, share, reps, seed):
    if share <= 0:
        raise ValueError('forward_share must be above zero so a session id can select the compressing arm')
    rng = random.Random(seed)
    config_hash = hashlib.sha256(json.dumps([model, effort, client_version]).encode()).hexdigest()
    runs = []
    for task in TASKS:
        for rep in range(1, reps + 1):
            arms = ['original', 'compact']
            rng.shuffle(arms)
            runs += [run_entry(task, rep, arm, share, rng, config_hash) for arm in arms]
    runs.append(run_entry(WEAKENED, 0, 'compact', share, rng, config_hash, 'weakened'))
    return {'schema': 'compressor-task-trial-plan-v1', 'model': model, 'effort': effort, 'client_version': client_version,
            'forward_share': share, 'reps': reps, 'seed': seed, 'cold_wait_seconds': COLD_WAIT_SECONDS, 'runs': runs}


def freeze_tree(task, root):
    build_tree(task, root)
    env = dict(os.environ, GIT_AUTHOR_DATE='2026-01-01T00:00:00Z', GIT_COMMITTER_DATE='2026-01-01T00:00:00Z',
               GIT_AUTHOR_NAME='trial', GIT_AUTHOR_EMAIL='trial@example.invalid',
               GIT_COMMITTER_NAME='trial', GIT_COMMITTER_EMAIL='trial@example.invalid')
    for command in (['init', '-q'], ['add', '-A'], ['commit', '-q', '-m', 'fixture']):
        subprocess.run(['git', *command], cwd=root, env=env, check=True, capture_output=True)
    return subprocess.run(['git', 'rev-parse', 'HEAD^{tree}'], cwd=root, check=True,
                          capture_output=True, text=True).stdout.strip()


class ClaudeAgent:
    def __init__(self, plan, proxy_url):
        self.plan, self.proxy_url = plan, proxy_url

    def invoke(self, run, root, prompt, resume):
        ident = ['--resume', run['session']] if resume else ['--session-id', run['session']]
        command = ['claude', '-p', prompt, *ident, '--model', self.plan['model'], '--effort', self.plan['effort'],
                   '--permission-mode', 'bypassPermissions', '--output-format', 'json',
                   '--setting-sources', 'project', '--strict-mcp-config']
        done = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=1800, check=False,
                              env=dict(os.environ, ANTHROPIC_BASE_URL=self.proxy_url))
        return {'exit': done.returncode, 'stdout': done.stdout, 'stderr': done.stderr}

    def stage1(self, run, root):
        return self.invoke(run, root, first_prompt(BY_ID[run['task_id']]), False)

    def stage2(self, run, root):
        return self.invoke(run, root, BY_ID[run['task_id']].prompt, True)


class ScriptedAgent:
    """Dry run: applies the reference solution so the plumbing is proved without a model."""

    def __init__(self, wrong=()):
        self.wrong = set(wrong)

    def stage1(self, _run, _root):
        return {'exit': 0, 'stdout': '{}', 'stderr': ''}

    def stage2(self, run, root):
        if run['task_id'] not in self.wrong and run['kind'] != 'weakened':
            BY_ID[run['task_id']].solve(root)
        return {'exit': 0, 'stdout': '{}', 'stderr': ''}


class Store:
    def __init__(self, directory):
        self.directory = Path(directory)
        self.lock = threading.Lock()

    def path(self, run):
        return self.directory / 'runs' / f"{run['task_id']}-{run['kind']}-{run['rep']}-{run['arm']}.json"

    def load(self, run):
        path = self.path(run)
        return json.loads(path.read_text()) if path.exists() else None

    def save(self, run, record):
        with self.lock:
            path = self.path(run)
            path.parent.mkdir(parents=True, exist_ok=True)
            temporary = path.with_suffix('.tmp')
            temporary.write_text(json.dumps(record, indent=1))
            temporary.replace(path)


def tree_for(store, run):
    return store.directory / 'trees' / store.path(run).stem


def first_stage(store, agent, run):
    if store.load(run):
        return
    root = tree_for(store, run)
    root.mkdir(parents=True)
    artifact = freeze_tree(BY_ID[run['task_id']], root)
    started = now_iso()
    outcome = agent.stage1(run, root)
    store.save(run, {**run, 'artifact_hash': artifact, 'started_at': started,
                     'stage1_finished_at': now_iso(), 'stage1': outcome})


def second_stage(store, agent, run):
    record = store.load(run)
    if not record or 'finished_at' in record or record['stage1']['exit'] != 0:
        return
    task, root = BY_ID[run['task_id']], tree_for(store, run)
    if run['kind'] == 'weakened':
        from .trial_tasks import weaken_section
        weaken_section(root)
    elif task.between:
        task.between(root)
    outcome = agent.stage2(run, root)
    verdict = grade(task, root) if outcome['exit'] == 0 else {'correct': False, 'failures': ['agent exited non-zero'], 'constraint_failures': []}
    store.save(run, {**record, 'finished_at': now_iso(), 'stage2': outcome, 'verdict': verdict})


def wait_awake(seconds, sleep=time.sleep):
    """Count on the monotonic clock in short steps; on macOS it stops during sleep, as the proxy's cache timer does."""
    deadline = time.monotonic() + max(0, seconds)
    while time.monotonic() < deadline:
        sleep(min(20, deadline - time.monotonic()))


def run_all(plan, directory, agent, concurrency=4, sleep=time.sleep):
    store = Store(directory)
    with ThreadPoolExecutor(concurrency) as pool:
        list(pool.map(lambda run: first_stage(store, agent, run), plan['runs']))
    finishes = [r['stage1_finished_at'] for r in map(store.load, plan['runs']) if r and 'finished_at' not in r]
    if finishes:
        latest = datetime.fromisoformat(max(finishes)).timestamp()
        wait_awake(latest + plan['cold_wait_seconds'] - time.time(), sleep)
    with ThreadPoolExecutor(concurrency) as pool:
        list(pool.map(lambda run: second_stage(store, agent, run), plan['runs']))
    return store


def turn_rows(capture):
    return [{'id': t['id'], 'input_tokens': (t['fresh'] or 0) + (t['cache_write'] or 0) + (t['cache_read'] or 0),
             'output_tokens': t['output'], 'buckets': {k: t[k] or 0 for k in WEIGHTS}, 'model': t['model']}
            for t in capture['turns'] if t['output'] is not None]


def units(turns):
    return sum(WEIGHTS[k] * t['buckets'][k] for t in turns for k in WEIGHTS)


def recoveries(capture, source):
    cut = [s['id'] for s in capture['stages'] if s['outcome'] == 'applied']
    if not cut:
        return 0
    return sum(1 for c in capture['calls'] if c['tool_name'] == 'Read' and (c['file_path'] or '').endswith(source)
               and c['request_id'] >= min(cut))


def assemble_run(record, capture, kind):
    task = BY_ID[record['task_id']]
    applied = [s for s in capture['stages'] if s['outcome'] == 'applied']
    run = {key: record[key] for key in ('task_hash', 'ground_truth_hash', 'artifact_hash', 'model_config_hash',
                                        'started_at', 'finished_at')}
    return {**run, 'execution_kind': kind, 'execution_ref': f"session:{record['session']}",
            'correct': record['verdict']['correct'], 'verdict_ref': f"grader:{record['ground_truth_hash'][:16]}",
            'constraint_verdict_ref': f"constraints:{record['ground_truth_hash'][:16]}",
            'verdict': record['verdict'], 'calls': [{'id': c['id'], 'content': c['content']} for c in capture['calls']],
            'model_turns': turn_rows(capture), 'required_facts': [],
            'activation_ref': f"session:{record['session']}:{'applied:' + ','.join(str(s['id']) for s in applied) if applied else 'no-cut'}",
            'cut_requests': len(applied), 'expected_cut': task.expects_cut,
            'hashes': [{'request': s['id'], 'original': s['original_sha256'], 'sent': s['sent_sha256']} for s in applied],
            'shadow_candidates': sum(s['reason'] == 'file_read_shadow_candidate' for s in capture['stages']),
            'recovery_reads': recoveries(capture, task.source)}


def pairs_from(plan, records, captures, kind):
    pairs = {}
    for run in plan['runs']:
        if run['kind'] != 'pair':
            continue
        record = records.get(run['session'])
        if not record or 'finished_at' not in record:
            continue
        pair = pairs.setdefault((run['task_id'], run['rep']), {'task_id': run['task_id'], 'family': run['family'], 'rep': run['rep']})
        pair[run['arm']] = assemble_run(record, captures[run['session']], kind)
    return [p for p in pairs.values() if 'original' in p and 'compact' in p]


def bootstrap_interval(values, seed=7, rounds=4000):
    if not values:
        return None
    rng = random.Random(seed)
    means = sorted(sum(rng.choices(values, k=len(values))) / len(values) for _ in range(rounds))
    return [means[int(rounds * 0.025)], means[int(rounds * 0.975)]]


def summarise(pairs):
    tasks = {}
    for pair in pairs:
        row = tasks.setdefault(pair['task_id'], {'family': pair['family'], 'o': 0.0, 'c': 0.0, 'ok_o': 0, 'ok_c': 0, 'n': 0,
                                                 'cuts': 0, 'recoveries': 0})
        row['o'] += units(pair['original']['model_turns'])
        row['c'] += units(pair['compact']['model_turns'])
        row['ok_o'] += pair['original']['correct']
        row['ok_c'] += pair['compact']['correct']
        row['n'] += 1
        row['cuts'] += pair['compact']['cut_requests'] > 0
        row['recoveries'] += pair['compact']['recovery_reads']
    per_task, deltas = {}, []
    for task_id, row in tasks.items():
        original = row['o'] / row['ok_o'] if row['ok_o'] else None
        compact = row['c'] / row['ok_c'] if row['ok_c'] else None
        delta = None if original is None or compact is None else (compact - original) / original
        if delta is not None:
            deltas.append(delta)
        per_task[task_id] = {'family': row['family'], 'repetitions': row['n'], 'correct_original': row['ok_o'],
                             'correct_compact': row['ok_c'], 'compact_runs_cut': row['cuts'], 'recovery_reads': row['recoveries'],
                             'cost_per_success_original': original, 'cost_per_success_compact': compact,
                             'relative_cost_change': delta}
    return {'tasks': per_task, 'task_count_with_cost': len(deltas),
            'mean_relative_cost_change': sum(deltas) / len(deltas) if deltas else None,
            'relative_cost_change_95_interval_over_tasks': bootstrap_interval(deltas),
            'cost_unit': 'input-equivalent tokens measured from provider buckets: fresh 1, cache write 1.25, cache read 0.1, output 5',
            'billing': 'not measured: sessions run on a subscription, so no per-call charge exists; no list-price estimate is made'}


def collect(plan, directory, capture_for, policy, kind='actual'):
    store = Store(directory)
    records = {r['session']: store.load(r) for r in plan['runs']}
    captures = {s: capture_for(s) for s, r in records.items() if r and 'finished_at' in r}
    pairs = pairs_from(plan, records, captures, kind)
    weakened_run = next(r for r in plan['runs'] if r['kind'] == 'weakened')
    weakened = records.get(weakened_run['session'])
    weakened_ok = bool(weakened and 'finished_at' in weakened and not weakened['verdict']['correct']
                       and any(s['outcome'] == 'applied' for s in captures[weakened_run['session']]['stages']))
    needing = [p for p in pairs if p['compact']['expected_cut']]
    recovered = [p for p in needing if p['compact']['cut_requests'] and p['compact']['recovery_reads']]
    recovery_ok = bool(recovered) and all(p['compact']['correct'] for p in recovered)
    finished = len([r for r in records.values() if r and 'finished_at' in r])
    return {'schema': SCHEMA, 'policy_sha256': policy_hash(policy), 'policy': policy, 'completed_at': now_iso(),
            'plan_sha256': hashlib.sha256(json.dumps(plan, sort_keys=True).encode()).hexdigest(),
            'runs_planned': len(plan['runs']), 'runs_finished': finished,
            'recovery': {'passed': recovery_ok, 'weakened_control_failed': weakened_ok,
                         'evidence_ref': f"plan:{plan['seed']}:runs:{len(recovered)}"},
            'pairs': pairs, 'summary': summarise(pairs)}


def read_capture(session, sql_runner):
    rows = sql_runner(CAPTURE_SQL.format(session=session)).strip()
    return json.loads(rows) if rows else {'turns': [], 'calls': [], 'stages': []}


def cut_share_of_expected(evidence):
    expected = [p['compact'] for p in evidence['pairs'] if p['compact']['expected_cut']]
    return sum(1 for run in expected if run['cut_requests'] > 0) / len(expected) if expected else 0.0


def verdict_of(evidence, policy):
    if cut_share_of_expected(evidence) < 0.5:
        return 'not a test of the compressor: fewer than half the compact runs that expect a cut were cut'
    return 'qualified' if qualifies(evidence, policy) else 'not qualified'
