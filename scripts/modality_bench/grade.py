"""Grade a cell from the tree that reached its origin, against the hidden answers. Reads
files and runs node; never reads the model's words."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from .ask import owned_paths, task_ids

GIT = ['git', '-c', 'user.name=modality-bench', '-c', 'user.email=bench@localhost', '-c', 'commit.gpgsign=false']
TRAPS = {
    # T5's brief names src/format/money.ts, which does not exist; the right move extends src/money.ts.
    'T5': lambda tree: [] if not (tree / 'src' / 'format').exists() else ['src/format/ was created'],
    # T3 reuses slugify from src/slug.ts rather than rewriting it.
    'T3': lambda tree: [] if 'slug' in read(tree / 'src' / 'title.ts') and 'import' in read(tree / 'src' / 'title.ts')
    else ['src/title.ts does not import from src/slug.ts'],
}


def read(path):
    try:
        return Path(path).read_text()
    except OSError:
        return ''


def run(argv, cwd, timeout=300):
    try:
        done = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return 124, f'timed out after {timeout} s'
    except OSError as err:
        return 127, str(err)
    return done.returncode, (done.stdout + done.stderr)[-2000:]


def checkout_landed(cell_dir, into):
    """origin.git's main, checked out into `into`; None when nothing is there."""
    origin = Path(cell_dir) / 'origin.git'
    code, out = run(['git', 'clone', '-q', '--branch', 'main', str(origin), str(into)], cwd=cell_dir)
    return None if code else Path(into)


def seed_commit(tree):
    code, out = run(['git', 'rev-list', '--max-parents=0', 'HEAD'], cwd=tree)
    return out.strip().splitlines()[-1] if code == 0 and out.strip() else None


def changed_paths(tree):
    base = seed_commit(tree)
    if base is None:
        return []
    code, out = run(['git', 'diff', '--name-only', base, 'HEAD'], cwd=tree)
    return sorted(p for p in out.splitlines() if p.strip()) if code == 0 else []


def link_modules(tree, work):
    modules = Path(work) / 'node_modules'
    target = Path(tree) / 'node_modules'
    if modules.is_dir() and not target.exists():
        os.symlink(modules, target)
    return (Path(tree) / 'node_modules' / '.bin' / 'tsc').exists()


def test_file_of(spec, task):
    return next((p for p in spec['answers'][task] if p.startswith('test/')), None)


def hidden_test(spec, cell_dir, tree, task):
    """Copy the known answer's test over the arm's and run it against the arm's src."""
    test_path = test_file_of(spec, task)
    answer = Path(cell_dir) / 'answers' / task / test_path
    if not answer.is_file():
        return False, f'no hidden test at {answer}'
    scratch = Path(tree) / 'test' / f'.hidden-{task}.test.ts'
    shutil.copy2(answer, scratch)
    try:
        code, out = run(['node', '--test', str(scratch.relative_to(tree))], cwd=tree, timeout=120)
    finally:
        scratch.unlink(missing_ok=True)
    return code == 0, out.strip().splitlines()[-1] if out.strip() else ''


def own_test(spec, tree, task):
    test_path = test_file_of(spec, task)
    if not (Path(tree) / test_path).is_file():
        return False, f'{test_path} was not written'
    code, out = run(['node', '--test', test_path], cwd=tree, timeout=120)
    return code == 0, out.strip().splitlines()[-1] if out.strip() else ''


def landed(spec, tree, task):
    return all((Path(tree) / p).is_file() for p in spec['answers'][task])


def grade_tree(spec, cell_dir, tree, work):
    """One tree graded: per task a verdict, plus the tree-level typecheck and scope."""
    tree = Path(tree)
    has_tsc = link_modules(tree, work)
    typecheck = {'ran': has_tsc}
    if has_tsc:
        code, out = run(['npm', 'run', '--silent', 'typecheck'], cwd=tree, timeout=300)
        typecheck.update(passed=code == 0, tail=out.strip().splitlines()[-1] if out.strip() else '')
    else:
        typecheck.update(passed=False, tail='node_modules/.bin/tsc is missing; the cell was prepared without an install')
    allowed = set(owned_paths(spec))
    changed = changed_paths(tree)
    outside = [p for p in changed if p not in allowed]
    tasks = {}
    for task in task_ids(spec):
        verdict = {'landed': landed(spec, tree, task)}
        if not verdict['landed']:
            verdict.update(accepted=False, failed='files did not land')
        else:
            ok, tail = hidden_test(spec, cell_dir, tree, task)
            verdict['hidden_test'] = {'passed': ok, 'tail': tail}
            own_ok, own_tail = own_test(spec, tree, task)
            verdict['own_test'] = {'passed': own_ok, 'tail': own_tail}
            traps = TRAPS.get(task, lambda _tree: [])(tree)
            verdict['traps'] = traps
            reasons = ([] if ok else ['hidden test']) + ([] if own_ok else ['own test']) + \
                      ([] if typecheck['passed'] else ['typecheck']) + ([] if not outside else ['scope']) + traps
            verdict['accepted'] = not reasons
            if reasons:
                verdict['failed'] = ', '.join(reasons)
        tasks[task] = verdict
    return {'typecheck': typecheck, 'changed_paths': changed, 'outside_scope': outside, 'tasks': tasks,
            'accepted': sum(1 for v in tasks.values() if v['accepted']),
            'landed': sum(1 for v in tasks.values() if v['landed']), 'of': len(tasks)}


def grade_cell(spec, cell_dir):
    """What reached the origin is the score; the working copy is graded too, separately."""
    cell_dir = Path(cell_dir)
    work = cell_dir / 'work'
    grade_dir = cell_dir / 'grade'
    if grade_dir.exists():
        shutil.rmtree(grade_dir)
    grade_dir.mkdir()
    result = {'of': len(task_ids(spec))}
    tree = checkout_landed(cell_dir, grade_dir / 'landed')
    if tree is None:
        result['origin'] = {'accepted': 0, 'landed': 0, 'of': result['of'], 'tasks': {}, 'note': 'nothing reached origin main'}
    else:
        result['origin'] = grade_tree(spec, cell_dir, tree, work)
    if work.is_dir():
        copy = grade_dir / 'tree'
        shutil.copytree(work, copy, symlinks=True, ignore=shutil.ignore_patterns('node_modules'))
        if (copy / '.git').exists():
            run(GIT + ['add', '-A'], cwd=copy)
            run(GIT + ['commit', '-q', '--allow-empty', '-m', 'bench: uncommitted work'], cwd=copy)
        result['in_tree'] = grade_tree(spec, cell_dir, copy, work)
    result['accepted'] = result['origin']['accepted']
    result['landed'] = result['origin']['landed']
    return result


def scripted_solve(spec, cell_dir, wrong=(), push=True):
    """Dry run: apply the known answers to the working copy, commit and push, like an arm that
    got everything right; `wrong` names tasks left undone."""
    cell_dir = Path(cell_dir)
    work = cell_dir / 'work'
    for task in task_ids(spec):
        if task in wrong:
            continue
        for rel in spec['answers'][task]:
            source = cell_dir / 'answers' / task / rel
            target = work / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
    run(GIT + ['add', '-A'], cwd=work)
    run(GIT + ['commit', '-q', '-m', 'bench: scripted answers'], cwd=work)
    if push:
        code, out = run(['git', 'push', '-q', 'origin', 'main'], cwd=work)
        return code == 0
    return True


def scratch_dir(prefix='modality-bench-'):
    return tempfile.mkdtemp(prefix=prefix)
