"""How each claude -p arm is launched: the argv, the environment and the sealed Claude Code
home. The fleet arm is launched through the inspect API in runner.py."""
import json
import os
from pathlib import Path
import shutil

from .ask import DENIED_TOOLS

CLAUDE_ARMS = ('single', 'subagents', 'team', 'workflow')
# Variables the sealed home must not inherit: they would point the arm back at the operator's
# Claude Code state or change which model answers.
STRIPPED_ENV = ('CLAUDE_CONFIG_DIR', 'CLAUDE_CODE_SUBAGENT_MODEL', 'ANTHROPIC_MODEL',
                'ANTHROPIC_BASE_URL', 'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS', 'CLAUDE_CODE_TEAM_NAME',
                'CLAUDE_CODE_TEAMMATE_COMMAND', 'CLAUDE_CODE_REMOTE_MEMORY_DIR', 'MIKE_RUN_ID')
# teammateMode lives in the global config file (.claude.json), not settings.json.
TEAM_GLOBAL_CONFIG = {'teammateMode': 'in-process'}


def claude_argv(arm, plan, cell, ask_text, budget_usd=None):
    if arm not in CLAUDE_ARMS:
        raise ValueError(f'{arm} is not a claude -p arm')
    argv = ['claude', '-p', ask_text, '--session-id', cell['session'],
            '--model', plan['model'], '--effort', plan['effort'],
            '--permission-mode', 'bypassPermissions', '--permission-prompts', 'none',
            '--output-format', 'json', '--setting-sources', 'project', '--strict-mcp-config',
            '--no-chrome']
    denied = DENIED_TOOLS[arm]
    if denied:
        argv += ['--disallowed-tools', ','.join(denied)]
    if budget_usd is not None:
        argv += ['--max-budget-usd', str(budget_usd)]
    return argv


def claude_env(arm, plan, cell_dir, proxy_url, base_env=None):
    env = {k: v for k, v in (base_env if base_env is not None else os.environ).items() if k not in STRIPPED_ENV}
    env.update({
        'CLAUDE_CONFIG_DIR': str(Path(cell_dir) / 'claude-home'),
        'CLAUDE_CODE_DISABLE_AUTO_MEMORY': '1',
        'CLAUDE_CODE_SUBAGENT_MODEL': plan['model'],
        'ANTHROPIC_BASE_URL': proxy_url,
        'DISABLE_AUTOUPDATER': '1',
        'MIKE_MODALITY_BENCH_CELL': Path(cell_dir).name,
    })
    if arm == 'team':
        env['CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS'] = '1'
    return env


def seed_home(cell_dir, source_home=None, global_config=None):
    """An empty Claude Code home for the cell, with only the sign-in copied from the operator's:
    credentials are read from CLAUDE_CONFIG_DIR, so a sealed home with none is signed out.
    Nothing else (settings, CLAUDE.md, memory, transcripts, MCP servers) comes along."""
    home = Path(cell_dir) / 'claude-home'
    if home.exists() and any(home.iterdir()):
        raise RuntimeError(f'{home} is not empty; a cell home is made once')
    home.mkdir(parents=True, exist_ok=True)
    source = Path(source_home or os.environ.get('CLAUDE_CONFIG_DIR') or Path.home() / '.claude')
    copied = []
    credentials = source / '.credentials.json'
    if credentials.is_file():
        shutil.copy2(credentials, home / '.credentials.json')
        os.chmod(home / '.credentials.json', 0o600)
        copied.append('.credentials.json')
    state = {'hasCompletedOnboarding': True, 'projects': {}, **(global_config or {})}
    source_state = source.parent / '.claude.json' if source.name == '.claude' else source / '.claude.json'
    if source_state.is_file():
        try:
            account = json.loads(source_state.read_text()).get('oauthAccount')
        except (OSError, ValueError):
            account = None
        if account:
            state['oauthAccount'] = account
            copied.append('.claude.json:oauthAccount')
    (home / '.claude.json').write_text(json.dumps(state, indent=2) + '\n')
    os.chmod(home / '.claude.json', 0o600)
    return {'home': str(home), 'copied': copied, 'source': str(source)}
