"""A frozen plan: the arms, the cells in a seeded order, a session id per cell, the model pins
and the configuration hashes a cell must match before it starts."""
import hashlib
import json
import random
import uuid

from .ask import ARMS, DENIED_TOOLS, MODALITY_LINES, render_ask

SCHEMA = 'modality-bench-plan-v1'
DEFAULT_CAP_SECONDS = 2400
WEIGHTS = {'fresh': 1.0, 'cache_write': 1.25, 'cache_read': 0.1, 'output': 5.0}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def session_id(rng):
    return str(uuid.UUID(int=rng.getrandbits(128), version=4))


def make_cells(arms, reps, rng):
    cells = [{'id': f'{arm}-{rep}', 'arm': arm, 'rep': rep, 'session': session_id(rng)}
             for arm in arms for rep in range(1, reps + 1)]
    rng.shuffle(cells)
    for order, cell in enumerate(cells, 1):
        cell['order'] = order
    return cells


def make_plan(spec, spec_path, model, effort, reps, seed, arms, environment, cap_seconds=DEFAULT_CAP_SECONDS):
    """`environment` is what the host looked like at plan time: client_version, settings_hash,
    user_scope_hash, proxy_url, inspect_url. A cell refuses to start when the live values differ."""
    arms = list(arms)
    unknown = [arm for arm in arms if arm not in ARMS]
    if unknown:
        raise ValueError(f'unknown arms: {", ".join(unknown)}')
    if reps < 1:
        raise ValueError('reps must be at least 1')
    rng = random.Random(seed)
    asks = {arm: render_ask(spec, arm) for arm in arms if arm != 'fleet'}
    return {
        'schema': SCHEMA,
        'spec_path': str(spec_path),
        'spec_hash': digest(spec),
        'model': model,
        'effort': effort,
        'reps': reps,
        'seed': seed,
        'arms': arms,
        'cap_seconds': cap_seconds,
        'modality_lines': {arm: MODALITY_LINES[arm] for arm in arms},
        'denied_tools': {arm: list(DENIED_TOOLS[arm]) for arm in arms},
        'ask_hashes': {arm: hashlib.sha256(text.encode()).hexdigest() for arm, text in asks.items()},
        'weights': WEIGHTS,
        'environment': environment,
        'cells': make_cells(arms, reps, rng),
    }


def cells_in_order(plan, only=None):
    cells = sorted(plan['cells'], key=lambda c: c['order'])
    return [c for c in cells if only is None or c['id'] in only]
