"""Full-task fixtures for paired compressor trials: deterministic files, reference solutions
and executable graders that read only the finished tree, never the model's answer."""
from dataclasses import dataclass
import hashlib
import inspect
from pathlib import Path
import subprocess
import sys
from typing import Callable, Optional

FAMILIES = ('documentation', 'configuration', 'test-output')
STEP_TOOL = """import sys
STEPS = {'1': 'inventory: fixture present', '2': 'clock: unchanged', '3': 'workspace: ready'}
print(STEPS.get(sys.argv[1] if len(sys.argv) > 1 else '', 'unknown step'))
"""
WORDS = ('alpha bravo cedar delta ember fjord grove harbor island juniper kestrel '
         'lantern meadow north orchard pine quarry river summit timber').split()


def filler(label, lines):
    out = []
    for n in range(lines):
        digest = hashlib.sha256(f'{label}:{n}'.encode()).digest()
        words = ' '.join(WORDS[b % len(WORDS)] for b in digest[:9])
        out.append(f'{label} note {n:03d}: {words}.')
    return out


def write(root, relative, lines):
    path = Path(root) / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('\n'.join(lines) + '\n')


def text(root, relative):
    path = Path(root) / relative
    return path.read_text() if path.exists() else ''


def substitute(root, relative, old, new):
    write(root, relative, text(root, relative).replace(old, new).splitlines())


@dataclass(frozen=True)
class Task:
    id: str
    family: str
    source: str
    prompt: str
    build: Callable
    solve: Callable
    check: Callable
    constraints: Callable
    expects_cut: bool = True
    between: Optional[Callable] = None
    first_step: str = ''


def only_changed(root, relative, expected):
    return [] if text(root, relative) == expected else [f'{relative} differs from the only allowed edit']


def page(head, middle, tail, label):
    return head + filler(label, 130) + middle + filler(label + ' later', 130) + tail


def build_port(root):
    write(root, 'docs/operations.md', page(
        ['# Operations', 'The listener binds port 7410 by default.'],
        ['## Firewall', 'Open inbound TCP 7410 on the private interface.'],
        ['## Health', 'Probe http://localhost:7410/health every 30 seconds.'], 'ops'))


def port_expected():
    return ('# Operations\nThe listener binds port 7455 by default.\n'
            + '\n'.join(filler('ops', 130))
            + '\n## Firewall\nOpen inbound TCP 7455 on the private interface.\n'
            + '\n'.join(filler('ops later', 130))
            + '\n## Health\nProbe http://localhost:7455/health every 30 seconds.\n')


def solve_port(root):
    substitute(root, 'docs/operations.md', '7410', '7455')


def check_port(root):
    body = text(root, 'docs/operations.md')
    return [] if '7410' not in body and body.count('7455') == 3 else ['port not replaced everywhere']


def constraints_port(root):
    return only_changed(root, 'docs/operations.md', port_expected())


def build_flag(root):
    write(root, 'docs/cli.md', page(
        ['# Command line', 'Use --retry-limit to bound attempts.'],
        ['## Pacing', 'Combine --retry-limit with --retry-limit-ms to pace retries.'],
        ['## Examples', 'worker --retry-limit 3 --queue main'], 'cli'))


def solve_flag(root):
    body = text(root, 'docs/cli.md').replace('--retry-limit-ms', '\0').replace('--retry-limit', '--max-retries')
    write(root, 'docs/cli.md', body.replace('\0', '--retry-limit-ms').splitlines())


def check_flag(root):
    body = text(root, 'docs/cli.md').replace('--retry-limit-ms', '')
    return [] if '--retry-limit' not in body and body.count('--max-retries') == 3 else ['flag not renamed everywhere']


def constraints_flag(root):
    body = text(root, 'docs/cli.md')
    return [] if '--retry-limit-ms' in body and len(body.splitlines()) == 266 else ['the -ms flag or other lines changed']


def build_section(root):
    write(root, 'docs/guide.md', page(
        ['# Guide', 'Read this before operating the service.'],
        ['## Backups', 'Backups are retained for 37 days in the cold bucket.'],
        ['## Support', 'Open a ticket with the on-call team.'], 'guide'))


def solve_section(root):
    write(root, 'docs/guide.md', (text(root, 'docs/guide.md')
                                  + '## Upgrading\nBackups are kept for 37 days.\n').splitlines())


def check_section(root):
    _, marker, after = text(root, 'docs/guide.md').partition('## Upgrading')
    return [] if marker and '37' in after else ['the Upgrading section lacks the retention value']


def constraints_section(root):
    body = text(root, 'docs/guide.md')
    return [] if body.startswith('# Guide\n') and '## Support' in body.partition('## Upgrading')[0] else ['earlier text changed']


def weaken_section(root):
    substitute(root, 'docs/guide.md', 'Backups are retained for 37 days in the cold bucket.',
               'See the retention register for details.')


VALIDATE_TIMEOUT = """import re
body = open('config/service.yaml').read()
limit = int(re.search(r'max_timeout_ms: (\\d+)', body).group(1))
values = [int(v) for v in re.findall(r'^\\s+timeout_ms: (\\d+)', body, re.M)]
raise SystemExit('invalid: a timeout is out of range' if any(v > limit for v in values) else 0)
"""
VALIDATE_PORTS = """import re
ports = re.findall(r'^\\s+port: (\\d+)', open('config/services.yaml').read(), re.M)
raise SystemExit('invalid: duplicate port' if len(ports) != len(set(ports)) else 0)
"""
VALIDATE_CACHE = """import re
body = open('config/app.toml').read().partition('[env.production]')[2]
raise SystemExit(0 if re.search(r'^cache_enabled = true', body, re.M) else 'invalid: production cache is off')
"""


def yaml_notes(label, count):
    return [f'# {line}' for line in filler(label, count)]


def build_timeout(root):
    write(root, 'tools/validate.py', VALIDATE_TIMEOUT.splitlines())
    write(root, 'config/service.yaml', ['limits:', '  max_timeout_ms: 30000'] + yaml_notes('svc', 130)
          + ['upstream:', '  timeout_ms: 90000', '  retries: 2'] + yaml_notes('svc later', 130)
          + ['downstream:', '  timeout_ms: 5000'])


def solve_timeout(root):
    substitute(root, 'config/service.yaml', 'timeout_ms: 90000', 'timeout_ms: 30000')


def validator(root):
    return subprocess.run([sys.executable, 'tools/validate.py'], cwd=root, capture_output=True,
                          timeout=30, check=False).returncode


def check_validates(root):
    return [] if validator(root) == 0 else ['the validator still fails']


def constraints_timeout(root):
    body = text(root, 'config/service.yaml')
    return [] if 'max_timeout_ms: 30000' in body and 'timeout_ms: 5000' in body and 'retries: 2' in body else ['a limit or neighbour changed']


def build_ports(root):
    write(root, 'tools/validate.py', VALIDATE_PORTS.splitlines())
    write(root, 'config/services.yaml', ['services:', '  ledger:', '    port: 8080'] + yaml_notes('net', 130)
          + ['  billing:', '    # moves to 8090 when the ledger keeps 8080', '    port: 8080']
          + yaml_notes('net later', 130) + ['  audit:', '    port: 8085'])


def solve_ports(root):
    body = text(root, 'config/services.yaml')
    head, marker, tail = body.partition('  billing:')
    write(root, 'config/services.yaml', (head + marker + tail.replace('port: 8080', 'port: 8090')).splitlines())


def check_ports(root):
    return check_validates(root) or ([] if text(root, 'config/services.yaml').partition('  billing:')[2].count('port: 8090') == 1
                                     else ['billing did not move to the noted port'])


def constraints_ports(root):
    body = text(root, 'config/services.yaml')
    return [] if body.partition('  billing:')[0].count('port: 8080') == 1 and 'port: 8085' in body else ['ledger or audit changed']


def build_cache(root):
    write(root, 'tools/validate.py', VALIDATE_CACHE.splitlines())
    write(root, 'config/app.toml', ['[defaults]', 'cache_enabled = true', 'workers = 4'] + yaml_notes('app', 130)
          + ['[env.production]', 'cache_enabled = false', 'workers = 16'] + yaml_notes('app later', 130)
          + ['[env.staging]', 'cache_enabled = false'])


def solve_cache(root):
    body = text(root, 'config/app.toml')
    head, marker, tail = body.partition('[env.production]')
    write(root, 'config/app.toml', (head + marker + tail.replace('cache_enabled = false', 'cache_enabled = true', 1)).splitlines())


def constraints_cache(root):
    body = text(root, 'config/app.toml')
    return [] if 'workers = 16' in body and body.partition('[env.staging]')[2].count('cache_enabled = false') == 1 else ['workers or staging changed']


def log_lines(label, count):
    return [f'PASS test_{label}_{n:03d} ({10 + n % 90} ms)' for n in range(count)]


def build_first_failure(root):
    write(root, 'logs/test-run.log', ['session starts', *log_lines('api', 140),
          'FAIL test_ledger_rounding: expected 1042 got 1043', *log_lines('api_late', 60),
          'FAIL test_export_header: expected csv got tsv', *log_lines('cli', 60), 'run finished'])


def solve_first_failure(root):
    write(root, 'triage.md', ['test_ledger_rounding: expected 1042 got 1043'])


def check_first_failure(root):
    body = text(root, 'triage.md')
    return [] if 'test_ledger_rounding' in body and '1042' in body and '1043' in body else ['first failure not reported']


def constraints_triage_only(root):
    return [] if 'test_export_header' not in text(root, 'triage.md') else ['triage names a later failure']


def build_real(root):
    write(root, 'logs/test-run.log', ['session starts', *log_lines('core', 120), 'FAIL test_lock_timeout: deadline exceeded',
          'RETRY test_lock_timeout: PASS on attempt 2', *log_lines('core_late', 40), 'FAIL test_invoice_total: expected 90 got 91',
          *log_lines('db', 40), 'FAIL test_tax_band: expected B got C', *log_lines('db_late', 60), 'run finished'])


def solve_real(root):
    write(root, 'triage.md', ['test_invoice_total', 'test_tax_band'])


def check_real(root):
    body = text(root, 'triage.md')
    return [] if 'test_invoice_total' in body and 'test_tax_band' in body else ['a real failure is missing']


def constraints_real(root):
    return [] if 'test_lock_timeout' not in text(root, 'triage.md') else ['triage lists the test that passed on retry']


def build_totals(root):
    write(root, 'logs/test-run.log', ['session starts', *log_lines('ui', 280), 'totals: 412 passed, 3 failed, 9 skipped'])


def solve_totals(root):
    write(root, 'triage.md', ['412 passed, 3 failed, 9 skipped'])


def check_totals(root):
    body = text(root, 'triage.md')
    return [] if '412' in body and '9' in body else ['totals not reported']


def no_constraint(_root):
    return []


def build_noop(root):
    write(root, 'docs/short.md', ['# Short note', 'The rotation interval is 14 days.'])


def solve_noop(root):
    write(root, 'answer.txt', ['14'])


def check_noop(root):
    return [] if '14' in text(root, 'answer.txt') else ['wrong interval']


def build_changed(root):
    write(root, 'docs/rotate.md', page(['# Rotation'], ['## Interval', 'Keys rotate every 14 days.'], ['## End'], 'rot'))


def change_interval(root):
    substitute(root, 'docs/rotate.md', 'Keys rotate every 14 days.', 'Keys rotate every 21 days.')


def solve_changed(root):
    write(root, 'answer.txt', ['21'])


def check_changed(root):
    return [] if '21' in text(root, 'answer.txt') else ['the stale interval was used']


def build_partial(root):
    write(root, 'docs/limits.md', page(['# Limits'], ['## Quota', 'The hourly quota is 640 requests.'], ['## End'], 'lim'))


def solve_partial(root):
    write(root, 'answer.txt', ['640'])


def check_partial(root):
    return [] if '640' in text(root, 'answer.txt') else ['wrong quota']


def build_repeat(root):
    write(root, 'docs/register.md', filler('reg', 90) + ['Primary region is eu-west-4.'] + filler('reg mid', 90)
          + ['Failover region is us-east-7.'] + filler('reg end', 90))


def solve_repeat(root):
    write(root, 'first.txt', ['eu-west-4'])
    write(root, 'second.txt', ['us-east-7'])


def check_repeat(root):
    return [] if 'eu-west-4' in text(root, 'first.txt') and 'us-east-7' in text(root, 'second.txt') else ['a region is wrong']


TASKS = (
    Task('doc-port', 'documentation', 'docs/operations.md',
         'Change the default listener port from 7410 to 7455 everywhere in docs/operations.md. Change nothing else.',
         build_port, solve_port, check_port, constraints_port),
    Task('doc-flag', 'documentation', 'docs/cli.md',
         'In docs/cli.md rename the flag --retry-limit to --max-retries everywhere. Leave --retry-limit-ms as it is.',
         build_flag, solve_flag, check_flag, constraints_flag),
    Task('doc-section', 'documentation', 'docs/guide.md',
         'Append a section headed "## Upgrading" to the end of docs/guide.md with one sentence saying how many days '
         'backups are retained, taken from what the guide says about backups.',
         build_section, solve_section, check_section, constraints_section),
    Task('cfg-timeout', 'configuration', 'config/service.yaml',
         'python3 tools/validate.py fails. Fix config/service.yaml so it passes, without changing any limit or any other setting.',
         build_timeout, solve_timeout, check_validates, constraints_timeout),
    Task('cfg-ports', 'configuration', 'config/services.yaml',
         'python3 tools/validate.py fails. Fix config/services.yaml so it passes, following the notes in that file. '
         'The ledger and audit services keep their ports.',
         build_ports, solve_ports, check_ports, constraints_ports),
    Task('cfg-cache', 'configuration', 'config/app.toml',
         'python3 tools/validate.py fails because production has its cache off. Fix config/app.toml so it passes. Staging stays off.',
         build_cache, solve_cache, check_validates, constraints_cache),
    Task('log-first', 'test-output', 'logs/test-run.log',
         'From logs/test-run.log, write to triage.md the name of the first failing test with its expected and actual values.',
         build_first_failure, solve_first_failure, check_first_failure, constraints_triage_only),
    Task('log-real', 'test-output', 'logs/test-run.log',
         'From logs/test-run.log, list in triage.md the tests that really failed, one per line. '
         'A test that passed on retry is not a real failure.',
         build_real, solve_real, check_real, constraints_real),
    Task('log-totals', 'test-output', 'logs/test-run.log',
         'From logs/test-run.log, write the final passed, failed and skipped totals to triage.md.',
         build_totals, solve_totals, check_totals, no_constraint),
    Task('ctl-noop', 'control-noop', 'docs/short.md',
         'Write the rotation interval in days, as a number, to answer.txt.',
         build_noop, solve_noop, check_noop, no_constraint, expects_cut=False),
    Task('ctl-changed', 'control-changed-file', 'docs/rotate.md',
         'docs/rotate.md may have changed since you read it. Write the current key rotation interval in days to answer.txt.',
         build_changed, solve_changed, check_changed, no_constraint, between=change_interval),
    Task('ctl-partial', 'control-partial-read', 'docs/limits.md',
         'Write the hourly quota, as a number, to answer.txt.',
         build_partial, solve_partial, check_partial, no_constraint,
         first_step='Read docs/limits.md with the Read tool, using offset 125 and limit 20 so you read only those lines.'),
    Task('ctl-repeat', 'control-repeated-recovery', 'docs/register.md',
         'Write the primary region to first.txt. When that is done, write the failover region to second.txt.',
         build_repeat, solve_repeat, check_repeat, no_constraint),
)
WEAKENED = next(t for t in TASKS if t.id == 'doc-section')
BY_ID = {task.id: task for task in TASKS}


def first_prompt(task):
    read = task.first_step or f'Read {task.source} in full with the Read tool.'
    return (f'{read} Then run python3 tools/step.py 1, then python3 tools/step.py 2, then python3 tools/step.py 3, '
            'one command per message. Change no files. When finished reply with the single word ready.')


def build_tree(task, root):
    write(root, 'tools/step.py', STEP_TOOL.splitlines())
    task.build(root)


def ground_truth_hash(task):
    source = ''.join(inspect.getsource(fn) for fn in (task.solve, task.check, task.constraints))
    return hashlib.sha256(source.encode()).hexdigest()


def task_hash(task):
    return hashlib.sha256('\0'.join((task.id, task.family, first_prompt(task), task.prompt)).encode()).hexdigest()


def grade(task, root):
    failures, broken = task.check(root), task.constraints(root)
    return {'correct': not failures and not broken, 'failures': failures, 'constraint_failures': broken}
