"""Explicit reusable commands with dependency-aware artifacts and no shell evaluation."""
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
import uuid

STATE = Path(os.environ.get('MIKE_TOOL_REUSE_STATE', Path.home() / '.local/state/mike-tool-reuse'))


def valid_id(value):
    if not re.fullmatch(r'[a-z0-9][a-z0-9_-]{0,79}', value):
        raise ValueError('Recipe ID must contain lowercase letters, digits, hyphens or underscores')
    return value


def digest(value):
    return hashlib.sha256(value).hexdigest()


def load(recipe_id):
    return json.loads((STATE / 'recipes' / (valid_id(recipe_id) + '.json')).read_text())


def register(args):
    argv = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not argv:
        raise ValueError('Supply an executable and arguments after --')
    if args.ttl < 0:
        raise ValueError('TTL must be nonnegative')
    recipe = {'id': valid_id(args.id), 'description': args.description,
              'cwd': str(Path(args.cwd).resolve(strict=True)), 'argv': argv,
              'dependencies': args.dependency, 'environment': args.environment,
              'ttl_seconds': args.ttl, 'contract': args.contract, 'version': 1}
    if not recipe['dependencies'] and recipe['ttl_seconds']:
        raise ValueError('Cached recipes require explicit dependencies; use --ttl 0 for live state')
    directory = STATE / 'recipes'
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / (recipe['id'] + '.json')
    with path.open('x') as file:
        json.dump(recipe, file, indent=2)
    return recipe


def dependency_state(recipe):
    cwd = Path(recipe['cwd'])
    state = []
    for pattern in sorted(recipe['dependencies']):
        if Path(pattern).is_absolute() or '..' in Path(pattern).parts:
            raise ValueError('Dependencies must be paths/globs inside the recipe working directory')
        matches = sorted(cwd.glob(pattern))
        state.append((pattern, 'matches', len(matches)))
        for path in matches:
            if path.is_symlink():
                raise ValueError(f'Symlink dependency requires a recipe rooted at its target: {path}')
            files = sorted(path.rglob('*')) if path.is_dir() else [path]
            for file in files:
                if file.is_symlink():
                    raise ValueError(f'Symlink dependency requires a recipe rooted at its target: {file}')
                if file.is_file():
                    state.append((str(file.relative_to(cwd)), digest(file.read_bytes()), file.stat().st_mode))
    return state


def identity(recipe):
    executable = recipe['argv'][0]
    import shutil
    resolved = shutil.which(executable) if '/' not in executable else str(Path(recipe['cwd']) / executable)
    if not resolved or not Path(resolved).is_file():
        raise ValueError('Recipe executable is unavailable')
    context = {'recipe': recipe, 'dependencies': dependency_state(recipe),
               'executable': digest(Path(resolved).read_bytes()),
               'environment': dict(os.environ)}
    return digest(json.dumps(context, sort_keys=True).encode())


def event(data):
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'events.jsonl').open('a') as file:
        fcntl.flock(file, fcntl.LOCK_EX)
        file.write(json.dumps({'at': dt.datetime.now(dt.timezone.utc).isoformat(), **data}) + '\n')


def run(recipe, force=False):
    for _ in range(3):
        key = identity(recipe)
        directory = STATE / 'results' / recipe['id'] / key
        directory.mkdir(parents=True, exist_ok=True)
        with (directory / 'lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            if identity(recipe) != key:
                continue
            return locked_run(recipe, key, directory, force)
    raise ValueError('Dependencies kept changing while waiting; rerun when inputs are stable')


def locked_run(recipe, key, directory, force):
    manifest = directory / 'result.json'
    previous = json.loads(manifest.read_text()) if manifest.exists() else None
    if previous and not force and recipe['ttl_seconds'] > 0 and time.time() - previous['completed_at'] < recipe['ttl_seconds']:
        event({'recipe': recipe['id'], 'kind': 'hit', 'key': key, 'exit_code': previous['exit_code']})
        return {**previous, 'reused': True}
    execution = directory / uuid.uuid4().hex
    execution.mkdir()
    started = time.monotonic()
    with (execution / 'stdout').open('wb') as stdout, (execution / 'stderr').open('wb') as stderr:
        result = subprocess.run(recipe['argv'], cwd=recipe['cwd'], stdout=stdout, stderr=stderr)
    unchanged = identity(recipe) == key
    record = {'recipe': recipe['id'], 'key': key, 'exit_code': result.returncode,
              'completed_at': time.time(), 'elapsed_seconds': time.monotonic() - started,
              'stdout': str(execution / 'stdout'), 'stderr': str(execution / 'stderr'),
              'reused': False, 'dependencies_unchanged': unchanged}
    (execution / 'receipt.json').write_text(json.dumps(record, indent=2))
    if unchanged and result.returncode == 0:
        temp = directory / 'result.tmp'
        temp.write_text(json.dumps(record, indent=2))
        os.replace(temp, manifest)
    elif manifest.exists():
        manifest.unlink()
    event({'recipe': recipe['id'], 'kind': 'run', 'key': key,
           'exit_code': result.returncode, 'elapsed_seconds': record['elapsed_seconds']})
    return record


def catalog():
    directory = STATE / 'recipes'
    return [json.loads(p.read_text()) for p in sorted(directory.glob('*.json'))]


def export(path):
    lines = ['# Validated reusable tool recipes', '',
             'Discover with `scripts/tool-reuse list`; inspect contracts with `show ID`; execute with `run ID`. '
             'Results report the original exit code and full stdout/stderr paths. Dependencies changing invalidate reuse.', '']
    for recipe in catalog():
        lines += [f"## {recipe['id']}", '', recipe['description'], '',
                  f"Working directory: `{recipe['cwd']}`", '', f"Contract: {recipe['contract']}", '',
                  f"Command: `{json.dumps(recipe['argv'])}`", '',
                  f"Dependencies: {json.dumps(recipe['dependencies'])}; TTL: {recipe['ttl_seconds']}s.", '']
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text('\n'.join(lines))
