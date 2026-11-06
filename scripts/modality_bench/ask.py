"""The one ask every arm reads, rendered from the reference fleet's definition, and the one
line per arm that names the modality. Nothing here is written by hand per run."""
import json
from pathlib import Path

ARMS = ('single', 'subagents', 'team', 'workflow', 'fleet')

MODALITY_LINES = {
    'single': 'Do all of this yourself in this session: do not start subagents, teams or workflows.',
    'subagents': 'Use the Agent tool to delegate the tasks to subagents, independent ones at the same time.',
    'team': 'Create an agent team and give each task to a teammate; the teammate for T3 waits for T1.',
    'workflow': 'Use a workflow to orchestrate these tasks with subagents.',
    'fleet': '',
}

# The tools each arm may not call. The denial makes the wrong modality impossible; the
# fidelity check (capture.py) makes a missing one visible.
DENIED_TOOLS = {
    'single': ('Agent', 'TeamCreate', 'Workflow'),
    'subagents': ('TeamCreate', 'Workflow'),
    'team': ('Workflow',),
    'workflow': ('TeamCreate',),
    'fleet': (),
}

FINISH_LINE = ('When every task is done, commit the work to the branch `main` of this checkout and '
               'push it to `origin`. Do not create branches, worktrees or other checkouts. '
               'Stop when the push has succeeded or when nothing more can be done.')


def load_spec(path):
    return json.loads(Path(path).read_text())


def task_block(task_id, task):
    lines = [f'### {task_id}: {task["name"]}', '', task['brief'], '',
             f'Files this task owns: {", ".join(task["files"])}.']
    new = task.get('new_files') or []
    if new:
        lines.append(f'Files it creates: {", ".join(new)}.')
    after = task.get('after') or []
    lines.append(f'Starts after: {", ".join(after)}.' if after else 'Starts at once.')
    return '\n'.join(lines)


def render_ask(spec, arm):
    """The prompt for a claude -p arm: goal, tasks, check, rules, finish line, modality line.
    The fleet arm never reads this text; its seats read the definition's briefs."""
    if arm not in ARMS:
        raise ValueError(f'unknown arm {arm!r}')
    definition = spec['definition']
    tasks = definition['tasks']
    parts = ['# The ask', '', definition['goal'], '',
             f'Five tasks, in the package in this directory. {len(tasks)} tasks in all; '
             'a task that starts after another needs that one finished first; the others are independent.',
             '', '## Tasks', '']
    for task_id in sorted(tasks):
        parts += [task_block(task_id, tasks[task_id]), '']
    check = definition.get('check') or {}
    parts += ['## Check', '',
              f'The project check is `{check.get("command", "scripts/check")}`: the type check, then every test. '
              'After an edit run `npm run typecheck`; one test is `node --test test/<name>.test.ts`.', '',
              '## Rules', '']
    parts += [f'- {rule}' for rule in definition.get('rules') or []]
    parts += ['', '## Finish', '', FINISH_LINE]
    line = MODALITY_LINES[arm]
    if line:
        parts += ['', '## How', '', line]
    return '\n'.join(parts) + '\n'


def task_ids(spec):
    return sorted(spec['definition']['tasks'])


def owned_paths(spec):
    """Every path any task may write: the union the scope check allows."""
    paths = set()
    for task in spec['definition']['tasks'].values():
        paths.update(task.get('files') or [])
        paths.update(task.get('new_files') or [])
    return sorted(paths)
