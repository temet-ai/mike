"""What must be true of mike and the host before a cell starts, and the hashes a plan pins.
Pure functions over fetched JSON, so the refusals are testable without a server."""
import hashlib
import json
from pathlib import Path
import subprocess

# A bench of Claude Code's modalities is not a bench of mike's steering: none of these may be on.
STEERING_TOGGLES = ('reminders', 'recall_injector', 'agent_lessons', 'workspace_briefing',
                    'self_identity_reminder', 'steerer', 'turn_guard')
# These rewrite what the model sees, for every arm alike, and so make it not Claude Code as shipped.
REWRITE_TOGGLES = ('weasel_intercept', 'proxy_tools', 'repeated_notices')
COMPRESSION = ('file_read_compression', 'shell_output_compression')
TRUE_VALUES = ('true', 'on', '1', 'enabled', 'yes')


def settings_map(payload):
    rows = payload.get('settings', payload) if isinstance(payload, dict) else payload
    if isinstance(rows, dict):
        return {str(k): v for k, v in rows.items()}
    return {str(row.get('key')): row.get('value') for row in rows if isinstance(row, dict)}


def is_on(value):
    if isinstance(value, bool):
        return value
    if isinstance(value, dict):
        return is_on(value.get('enabled', value.get('on', False)))
    return str(value).strip().lower() in TRUE_VALUES


def toggle_on(settings, key):
    for candidate in (key, f'feature_toggles.{key}', f'{key}.enabled'):
        if candidate in settings:
            return is_on(settings[candidate])
    return False


def settings_refusals(settings, filter_rule_count):
    """Why a run may not start, given the live settings; empty means it may."""
    refusals = [f'{key} is on' for key in STEERING_TOGGLES + REWRITE_TOGGLES if toggle_on(settings, key)]
    if is_on(settings.get('reminders.inject_lessons', False)):
        refusals.append('reminders.inject_lessons is on')
    for key in COMPRESSION:
        share = settings.get(f'{key}.forward_share')
        if toggle_on(settings, key) and share not in (None, 0, '0', 0.0, '0.0'):
            refusals.append(f'{key} forwards cuts (forward_share {share})')
    if filter_rule_count:
        refusals.append(f'{filter_rule_count} filter rule(s) exist')
    return refusals


def settings_hash(settings):
    return hashlib.sha256(json.dumps(settings, sort_keys=True, default=str).encode()).hexdigest()


def filter_rule_count(payload):
    rows = payload.get('rules', payload) if isinstance(payload, dict) else payload
    return len(rows) if isinstance(rows, list) else 0


def running_fleets(payload):
    rows = payload.get('fleets', payload) if isinstance(payload, dict) else payload
    if not isinstance(rows, list):
        return []
    return [row.get('slug') or row.get('fleet_uuid') or '?' for row in rows
            if isinstance(row, dict) and row.get('state') in ('starting', 'running', 'stopping')]


def host_refusals(deploy_lock, fleets_payload):
    refusals = []
    if Path(deploy_lock).exists():
        refusals.append(f'a deploy holds {deploy_lock}')
    running = running_fleets(fleets_payload)
    if running:
        refusals.append(f'fleet(s) running: {", ".join(running)}')
    return refusals


def user_scope_hash(claude_home):
    """The operator's user-scope Claude Code files a fleet seat sees and the other arms do not."""
    home = Path(claude_home)
    digest = hashlib.sha256()
    found = []
    for name in ('settings.json', 'CLAUDE.md'):
        path = home / name
        if path.is_file():
            digest.update(name.encode() + b'\0' + path.read_bytes() + b'\0')
            found.append(name)
    return {'hash': digest.hexdigest(), 'files': found}


def client_version():
    try:
        return subprocess.run(['claude', '--version'], capture_output=True, text=True, check=True, timeout=30).stdout.strip()
    except (OSError, subprocess.SubprocessError) as err:
        raise SystemExit(f'claude --version failed: {err}')


def host_snapshot():
    snap = {}
    try:
        snap['loadavg'] = Path('/proc/loadavg').read_text().split()[:3]
    except OSError:
        pass
    try:
        for line in Path('/proc/meminfo').read_text().splitlines():
            if line.startswith('MemAvailable:'):
                snap['mem_available_kb'] = int(line.split()[1])
    except OSError:
        pass
    import os
    snap['nproc'] = os.cpu_count()
    return snap
