"""Which of the compressor's protected-path globs would hide a trial's source file (temet-ai/mike-private#694)."""
from pathlib import Path
import re

from .trial_tasks import TASKS


def glob_regex(pattern):
    parts, index = [], 0
    while index < len(pattern):
        if pattern.startswith('**/', index):
            parts.append('(?:.*/)?')
            index += 3
        elif pattern.startswith('**', index):
            parts.append('.*')
            index += 2
        elif pattern[index] == '*':
            parts.append('[^/]*')
            index += 1
        elif pattern[index] == '?':
            parts.append('[^/]')
            index += 1
        else:
            parts.append(re.escape(pattern[index]))
            index += 1
    return re.compile(''.join(parts) + r'\Z')


def matching_glob(patterns, path):
    return next((pattern for pattern in patterns or [] if glob_regex(pattern).match(path)), None)


def refusals(policy, directory):
    root = Path(directory).resolve() / 'trees' / 'any-run'
    found = []
    for task in TASKS:
        path = str(root / task.source)
        pattern = matching_glob(policy.get('protected_paths'), path)
        if pattern:
            found.append((pattern, path))
    return found


def refuse_protected_directory(policy, directory):
    found = refusals(policy, directory)
    if found:
        pattern, path = found[0]
        raise SystemExit(
            f'{directory} would hide every trial file from the compressor: {path} matches the protected path '
            f'{pattern!r}, so nothing would be cut. Use a directory outside it ({len(found)} of {len(TASKS)} task files match).')
