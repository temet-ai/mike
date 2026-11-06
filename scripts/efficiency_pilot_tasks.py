"""Small, offline, independently graded cross-stack tasks; no captured user content."""
import subprocess
from pathlib import Path


TASKS = {
    'python': dict(file='settings.py', symbol='choose',
        prompt='Fix choose so an explicit value wins over the fallback, including 0, False and empty text. Only None means absent. Preserve the function signature.',
        before='def choose(value, fallback):\n    return value or fallback\n',
        after='def choose(value, fallback):\n    return fallback if value is None else value\n',
        grader="from settings import choose\nassert choose(None, 9) == 9\nassert choose(0, 9) == 0\nassert choose(False, True) is False\nassert choose('', 'x') == ''\nassert choose('a', 'x') == 'a'\n",
        command=['python3', '-B', 'verify.py']),
    'typescript': dict(file='settings.ts', symbol='choose',
        prompt='Fix choose so explicit values including 0, false and empty text win. Null and undefined use the fallback. Preserve its exported signature.',
        before='export function choose<T>(value: T | null | undefined, fallback: T): T {\n  return value || fallback;\n}\n',
        after='export function choose<T>(value: T | null | undefined, fallback: T): T {\n  return value ?? fallback;\n}\n',
        grader="import { choose } from './settings.ts';\nimport assert from 'node:assert/strict';\nassert.equal(choose(0, 9), 0);\nassert.equal(choose(false, true), false);\nassert.equal(choose('', 'x'), '');\nassert.equal(choose(null, 9), 9);\nassert.equal(choose(undefined, 9), 9);\nassert.equal(choose('a', 'x'), 'a');\n",
        command=['node', 'verify.ts']),
    'rust': dict(file='settings.rs', symbol='choose',
        prompt='Fix choose so Some(0) is an explicit value. Only None uses the fallback. Preserve the public signature.',
        before='pub fn choose(value: Option<u32>, fallback: u32) -> u32 {\n    value.filter(|v| *v > 0).unwrap_or(fallback)\n}\n',
        after='pub fn choose(value: Option<u32>, fallback: u32) -> u32 {\n    value.unwrap_or(fallback)\n}\n',
        grader='mod settings;\n#[test] fn explicit_values() { assert_eq!(settings::choose(Some(0), 9), 0); assert_eq!(settings::choose(Some(3), 9), 3); assert_eq!(settings::choose(None, 9), 9); }\n',
        command=['rustc', '--test', 'verify.rs', '-o', 'verify']),
    'documentation': dict(file='RUNBOOK.md', symbol=None,
        prompt='Correct the runbook for this Python project. The single verification command is python3 -m unittest discover -s tests. Run it from the repository root, offline. State that exit code 0 means the tests passed and nonzero means failure. Remove Cargo instructions. Keep the exact heading # Verification.',
        before='# Verification\nRun cargo test from any directory. Success is inferred from the final output line.\n',
        after='# Verification\nFrom the repository root, run offline:\n\n```sh\npython3 -m unittest discover -s tests\n```\n\nExit code 0 means tests passed; a nonzero exit code means failure.\n',
        grader="from pathlib import Path\ns = Path('RUNBOOK.md').read_text().lower()\nassert s.startswith('# verification\\n')\nassert 'python3 -m unittest discover -s tests' in s\nassert 'repository root' in s and 'offline' in s\nassert 'exit code 0' in s and 'nonzero' in s and 'failure' in s\nassert 'cargo' not in s\nassert 'final output line' not in s\n",
        command=['python3', '-B', 'verify.py']),
}


def task_files(task, solved=False):
    if 'files' in task:
        return {**task['files'], **(task.get('solution', {}) if solved else {})}
    return {task['file']: task['after'] if solved else task['before']}


def write_files(root, files):
    for name, text in files.items():
        relative = Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('task paths must stay inside the workspace')
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)


def grade(task, workspace, destination):
    """Grade only permitted edits; contracts and graders remain trusted."""
    destination.mkdir(parents=True)
    files = task_files(task)
    editable = task.get('editable_files', list(task.get('solution', {}))) or [task['file']]
    for name, expected in files.items():
        if name not in editable:
            candidate = workspace / name
            if not candidate.is_file() or candidate.is_symlink() or not candidate.resolve().is_relative_to(workspace.resolve()) or candidate.read_text() != expected:
                return dict(correct=False, reason='fixed fixture changed')
    for name in editable:
        candidate = workspace / name
        if not candidate.is_file() or candidate.is_symlink() or not candidate.resolve().is_relative_to(workspace.resolve()):
            return dict(correct=False, reason='missing or escaping candidate')
        files[name] = candidate.read_text()
    write_files(destination, files)
    extension = {'rustc': 'rs', 'node': 'ts'}.get(task['command'][0], 'py')
    (destination / ('verify.' + extension)).write_text(task['grader'])
    commands = [task['command']]
    if extension == 'rs':
        commands.append([str((destination / 'verify').resolve())])
    logs = []
    for command in commands:
        try:
            result = subprocess.run(command, cwd=destination, text=True, capture_output=True, timeout=30)
        except subprocess.TimeoutExpired:
            return dict(correct=False, reason='grader timeout', logs=logs)
        logs.append(dict(command=command, exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
        if result.returncode:
            return dict(correct=False, logs=logs)
    return dict(correct=True, logs=logs)
