"""scripts/modality-bench: the pure parts (ask, plan, argv, refusals, fidelity, totals) and the
grader against a prepared cell. No model, no database, no proxy."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from modality_bench import arms, ask, capture, grade, plan as planning, preflight, report  # noqa: E402

SPEC = ask.load_spec(ROOT / 'scripts' / 'fleet-refs' / 'node.json')
ENV = {'client_version': '2.1.319 (Claude Code)', 'settings_hash': 'abc', 'proxy_url': 'http://p', 'inspect_url': 'http://i'}


class Ask(unittest.TestCase):
    def test_every_claude_arm_reads_the_same_tasks_and_differs_only_in_its_modality_line(self):
        texts = {arm: ask.render_ask(SPEC, arm) for arm in ask.ARMS}
        for arm in ('single', 'subagents', 'team', 'workflow'):
            self.assertIn(ask.MODALITY_LINES[arm], texts[arm])
            self.assertTrue(texts[arm].endswith(ask.MODALITY_LINES[arm] + '\n'))
        self.assertNotIn('## How', texts['fleet'])
        bodies = {text.split('## How')[0].rstrip() for text in texts.values()}
        self.assertEqual(len(bodies), 1, 'the ask before the modality line is identical across arms')

    def test_the_ask_carries_every_brief_verbatim_and_the_dependency(self):
        text = ask.render_ask(SPEC, 'single')
        for task in SPEC['definition']['tasks'].values():
            self.assertIn(task['brief'], text)
        self.assertIn('Starts after: T1.', text)
        self.assertIn(ask.FINISH_LINE, text)

    def test_an_unknown_arm_is_refused(self):
        with self.assertRaises(ValueError):
            ask.render_ask(SPEC, 'swarm')

    def test_owned_paths_is_the_union_of_files_and_new_files(self):
        paths = ask.owned_paths(SPEC)
        self.assertIn('src/money.ts', paths)
        self.assertIn('test/money-currency.test.ts', paths)
        self.assertNotIn('src/index.ts', paths)


class Plan(unittest.TestCase):
    def test_a_seed_fixes_the_order_and_the_session_ids(self):
        one = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 2, 7, ask.ARMS, ENV)
        two = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 2, 7, ask.ARMS, ENV)
        self.assertEqual(one['cells'], two['cells'])
        self.assertEqual(len(one['cells']), 10)
        self.assertEqual(len({c['session'] for c in one['cells']}), 10)
        self.assertEqual(sorted(c['order'] for c in one['cells']), list(range(1, 11)))

    def test_another_seed_changes_the_order(self):
        one = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 3, 1, ask.ARMS, ENV)
        two = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 3, 2, ask.ARMS, ENV)
        self.assertNotEqual([c['id'] for c in planning.cells_in_order(one)], [c['id'] for c in planning.cells_in_order(two)])

    def test_unknown_arms_and_zero_reps_are_refused(self):
        with self.assertRaises(ValueError):
            planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 1, 1, ['single', 'swarm'], ENV)
        with self.assertRaises(ValueError):
            planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 0, 1, ['single'], ENV)

    def test_the_plan_pins_the_ask_the_denials_and_the_environment(self):
        plan = planning.make_plan(SPEC, 'x', 'opus', 'high', 1, 1, ['single', 'fleet'], ENV, cap_seconds=60)
        self.assertEqual(plan['ask_hashes'].keys(), {'single'})
        self.assertEqual(plan['denied_tools']['single'], ['Agent', 'TeamCreate', 'Workflow'])
        self.assertEqual(plan['environment'], ENV)
        self.assertEqual(plan['cap_seconds'], 60)


class Argv(unittest.TestCase):
    def setUp(self):
        self.plan = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 1, 1, ask.ARMS, ENV)
        self.cell = {'id': 'single-1', 'arm': 'single', 'rep': 1, 'session': '11111111-1111-4111-8111-111111111111'}

    def test_every_claude_arm_is_pinned_sealed_and_unattended(self):
        for arm in arms.CLAUDE_ARMS:
            argv = arms.claude_argv(arm, self.plan, self.cell, 'ASK')
            for flag in ('--session-id', '--model', '--effort', '--strict-mcp-config', '--permission-prompts', '--setting-sources'):
                self.assertIn(flag, argv, arm)
            self.assertEqual(argv[argv.index('--setting-sources') + 1], 'project')
            self.assertEqual(argv[argv.index('--permission-prompts') + 1], 'none')
            self.assertEqual(argv[argv.index('--model') + 1], 'sonnet')

    def test_denied_tools_follow_the_arm(self):
        single = arms.claude_argv('single', self.plan, self.cell, 'ASK')
        self.assertEqual(single[single.index('--disallowed-tools') + 1], 'Agent,TeamCreate,Workflow')
        team = arms.claude_argv('team', self.plan, self.cell, 'ASK')
        self.assertEqual(team[team.index('--disallowed-tools') + 1], 'Workflow')
        self.assertNotIn('--settings', team)

    def test_the_fleet_arm_has_no_claude_argv(self):
        with self.assertRaises(ValueError):
            arms.claude_argv('fleet', self.plan, self.cell, 'ASK')

    def test_the_environment_seals_the_home_and_pins_the_subagent_model(self):
        base = {'PATH': '/bin', 'HOME': '/h', 'CLAUDE_CONFIG_DIR': '/h/.claude', 'ANTHROPIC_MODEL': 'x',
                'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS': '1', 'MIKE_RUN_ID': 'fleet-x'}
        env = arms.claude_env('single', self.plan, '/cells/single-1', 'http://proxy', base)
        self.assertEqual(env['CLAUDE_CONFIG_DIR'], '/cells/single-1/claude-home')
        self.assertEqual(env['CLAUDE_CODE_SUBAGENT_MODEL'], 'sonnet')
        self.assertEqual(env['ANTHROPIC_BASE_URL'], 'http://proxy')
        self.assertEqual(env['CLAUDE_CODE_DISABLE_AUTO_MEMORY'], '1')
        for stripped in ('ANTHROPIC_MODEL', 'CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS', 'MIKE_RUN_ID'):
            self.assertNotIn(stripped, env)
        self.assertEqual(arms.claude_env('team', self.plan, '/c', 'http://p', base)['CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS'], '1')

    def test_a_seeded_home_holds_only_the_sign_in(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / 'source' / '.claude'
            source.mkdir(parents=True)
            (source / '.credentials.json').write_text('{"secret": 1}')
            (source / 'settings.json').write_text('{"hooks": {}}')
            (source / 'CLAUDE.md').write_text('operator notes')
            (source.parent / '.claude.json').write_text(json.dumps({'oauthAccount': {'emailAddress': 'x'}, 'projects': {'/a': {}}}))
            cell = Path(tmp) / 'cell'
            seeded = arms.seed_home(cell, source_home=source, global_config=arms.TEAM_GLOBAL_CONFIG)
            home = cell / 'claude-home'
            self.assertEqual(sorted(p.name for p in home.iterdir()), ['.claude.json', '.credentials.json'])
            state = json.loads((home / '.claude.json').read_text())
            self.assertEqual(state['teammateMode'], 'in-process')
            self.assertEqual(state['projects'], {})
            self.assertEqual(state['oauthAccount'], {'emailAddress': 'x'})
            self.assertEqual(oct(os.stat(home / '.credentials.json').st_mode & 0o777), '0o600')
            self.assertIn('.credentials.json', seeded['copied'])
            with self.assertRaises(RuntimeError):
                arms.seed_home(cell, source_home=source)


class Refusals(unittest.TestCase):
    def test_steering_rewrites_and_filter_rules_refuse(self):
        settings = {'recall_injector': 'true', 'weasel_intercept': {'enabled': True}, 'reminders.inject_lessons': True,
                    'file_read_compression': 'on', 'file_read_compression.forward_share': 20}
        refusals = preflight.settings_refusals(settings, filter_rule_count=2)
        text = '\n'.join(refusals)
        for expected in ('recall_injector is on', 'weasel_intercept is on', 'reminders.inject_lessons is on',
                         'file_read_compression forwards cuts', '2 filter rule(s) exist'):
            self.assertIn(expected, text)

    def test_shadow_compression_and_off_toggles_pass(self):
        settings = {'recall_injector': 'false', 'file_read_compression': 'on', 'file_read_compression.forward_share': 0,
                    'change_record': True, 'launch_brief': True}
        self.assertEqual(preflight.settings_refusals(settings, 0), [])

    def test_settings_rows_and_maps_both_read(self):
        rows = {'settings': [{'key': 'steerer', 'value': 'true'}]}
        self.assertEqual(preflight.settings_map(rows), {'steerer': 'true'})
        self.assertEqual(preflight.settings_refusals(preflight.settings_map(rows), 0), ['steerer is on'])

    def test_running_fleets_and_a_deploy_lock_refuse(self):
        with tempfile.TemporaryDirectory() as tmp:
            lock = Path(tmp) / 'deploy.lock'
            self.assertEqual(preflight.host_refusals(lock, {'fleets': [{'slug': 'a', 'state': 'done'}]}), [])
            lock.touch()
            refusals = preflight.host_refusals(lock, [{'slug': 'b', 'state': 'running'}, {'slug': 'c', 'state': 'draft'}])
            self.assertEqual(len(refusals), 2)
            self.assertIn('fleet(s) running: b', refusals[1])

    def test_the_user_scope_hash_names_the_files_it_saw(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(preflight.user_scope_hash(tmp)['files'], [])
            (Path(tmp) / 'CLAUDE.md').write_text('x')
            first = preflight.user_scope_hash(tmp)
            self.assertEqual(first['files'], ['CLAUDE.md'])
            (Path(tmp) / 'CLAUDE.md').write_text('y')
            self.assertNotEqual(first['hash'], preflight.user_scope_hash(tmp)['hash'])


def snapshot(sessions=1, lanes=(), tools=(), seats=0):
    return {'sessions': sessions, 'seat_launches': seats,
            'lanes': [{'lane': code, 'requests': n, 'fresh': 10, 'cache_write': 100, 'cache_read': 1000, 'output': 20} for code, n in lanes],
            'tools': [{'tool_name': name, 'uses': n} for name, n in tools]}


class Fidelity(unittest.TestCase):
    def test_each_arm_is_faithful_only_when_its_modality_fired(self):
        cases = {
            'single': (snapshot(lanes=[('primary', 5)], tools=[('Bash', 3)]), snapshot(lanes=[('primary', 5), ('subagent', 2)], tools=[('Agent', 1)])),
            'subagents': (snapshot(lanes=[('primary', 5), ('subagent', 4)], tools=[('Agent', 2)]), snapshot(lanes=[('primary', 5)], tools=[('Bash', 1)])),
            'team': (snapshot(sessions=3, lanes=[('primary', 5)], tools=[('TeamCreate', 1)]), snapshot(lanes=[('primary', 5)], tools=[('Bash', 1)])),
            'workflow': (snapshot(lanes=[('primary', 5), ('workflow_subagent', 4)], tools=[('Workflow', 1)]), snapshot(lanes=[('primary', 5), ('subagent', 4)], tools=[('Agent', 1)])),
            'fleet': (snapshot(sessions=6, lanes=[('primary', 30)], seats=5), snapshot(sessions=1, lanes=[('primary', 5)])),
        }
        for arm, (good, bad) in cases.items():
            self.assertTrue(capture.fidelity(arm, good)[0], arm)
            ok, reasons = capture.fidelity(arm, bad)
            self.assertFalse(ok, arm)
            self.assertTrue(reasons, arm)

    def test_nothing_captured_is_never_faithful(self):
        ok, reasons = capture.fidelity('single', snapshot(sessions=0))
        self.assertFalse(ok)
        self.assertIn('nothing captured at the project path', reasons)

    def test_totals_sum_every_lane_and_weight_them(self):
        totals = capture.totals(snapshot(lanes=[('primary', 2), ('subagent', 3)], tools=[('Bash', 4), ('Read', 1)]))
        self.assertEqual(totals['requests'], 5)
        self.assertEqual(totals['fresh'], 20)
        self.assertEqual(totals['tool_calls'], 5)
        self.assertEqual(totals['weighted'], round(20 * 1.0 + 200 * 1.25 + 2000 * 0.1 + 40 * 5.0))

    def test_a_quoted_project_path_is_not_queried(self):
        with self.assertRaises(ValueError):
            capture.read_cell('postgres://x', "/a/b'c")


class Report(unittest.TestCase):
    def record(self, arm, rep, accepted, fidelity=True, wall=100):
        return {'id': f'{arm}-{rep}', 'arm': arm, 'rep': rep, 'stop': 'done', 'wall_seconds': wall,
                'fidelity': {'ok': fidelity}, 'grade': {'accepted': accepted, 'landed': 5, 'of': 5, 'in_tree': {'accepted': accepted}},
                'totals': {'requests': 10, 'tool_calls': 20, 'fresh': 1, 'cache_write': 2, 'cache_read': 3, 'output': 4, 'weighted': 30, 'sessions': 1}}

    def test_medians_use_only_faithful_cells_and_need_three(self):
        plan = planning.make_plan(SPEC, 'x', 'sonnet', 'medium', 3, 1, ['single', 'team'], ENV)
        records = [self.record('single', 1, 5), self.record('single', 2, 3, wall=300), self.record('single', 3, 4, wall=200),
                   self.record('team', 1, 5), self.record('team', 2, 5), self.record('team', 3, 0, fidelity=False)]
        results = report.build_results(plan, records)
        self.assertEqual(results['arms']['single']['medians']['accepted'], 4)
        self.assertEqual(results['arms']['single']['ranges']['wall_seconds'], [100, 300])
        self.assertEqual(results['arms']['team']['faithful'], 2)
        self.assertIn('note', results['arms']['team'])
        text = report.render_report(results)
        self.assertIn('| single-1 |', text)
        self.assertIn('Channels that stayed open', text)
        self.assertIn(ask.MODALITY_LINES['team'], text)


@unittest.skipUnless(shutil.which('node') and shutil.which('git'), 'node and git are needed')
class Grader(unittest.TestCase):
    """A cell prepared by scripts/fleet-ref without an install: hidden tests run, the typecheck
    cannot, so nothing is accepted and the reason is named."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix='modality-bench-test-')
        cls.cell = Path(cls.tmp) / 'single-1'
        env = dict(os.environ, FLEET_REF_NO_INSTALL='1')
        done = subprocess.run([str(ROOT / 'scripts' / 'fleet-ref'), 'prepare', 'node', '--dir', str(cls.cell)],
                              capture_output=True, text=True, env=env, check=False, timeout=300)
        if done.returncode != 0:
            raise unittest.SkipTest(f'fleet-ref prepare failed: {done.stderr[-300:]}')

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_nothing_pushed_grades_zero_and_the_working_copy_apart(self):
        result = grade.grade_cell(SPEC, self.cell)
        self.assertEqual(result['accepted'], 0)
        self.assertEqual(result['origin']['landed'], 0)
        self.assertEqual(result['in_tree']['landed'], 0)

    def test_the_hidden_test_catches_a_plain_math_round_and_the_wrong_money_file(self):
        work = self.cell / 'work'
        grade.scripted_solve(SPEC, self.cell, wrong=('T4', 'T5'), push=False)
        (work / 'src' / 'round.ts').write_text(
            'export function roundTo(value: number, digits: number): number {\n'
            '  const f = 10 ** digits; return Math.round(value * f) / f;\n}\n')
        (work / 'test' / 'round.test.ts').write_text(
            'import { test } from "node:test";\nimport assert from "node:assert/strict";\n'
            'import { roundTo } from "../src/round.ts";\ntest("rounds", () => { assert.equal(roundTo(1.26, 1), 1.3); });\n')
        (work / 'src' / 'format').mkdir(exist_ok=True)
        (work / 'src' / 'format' / 'money.ts').write_text('export function formatMoney(c: number, cur: string) { return cur; }\n')
        (work / 'src' / 'money.ts').write_text((self.cell / 'answers' / 'T5' / 'src' / 'money.ts').read_text())
        shutil.copy2(self.cell / 'answers' / 'T5' / 'test' / 'money-currency.test.ts', work / 'test' / 'money-currency.test.ts')
        subprocess.run(grade.GIT + ['add', '-A'], cwd=work, check=True, capture_output=True)
        subprocess.run(grade.GIT + ['commit', '-q', '-m', 'wrong answers'], cwd=work, check=True, capture_output=True)
        subprocess.run(['git', 'push', '-q', 'origin', 'main'], cwd=work, check=True, capture_output=True)
        result = grade.grade_cell(SPEC, self.cell)
        tasks = result['origin']['tasks']
        self.assertEqual(result['origin']['landed'], 5)
        self.assertFalse(result['origin']['typecheck']['ran'])
        self.assertTrue(tasks['T1']['hidden_test']['passed'])
        self.assertIn('typecheck', tasks['T1']['failed'])
        self.assertFalse(tasks['T4']['hidden_test']['passed'], 'half away from zero is the hidden rule')
        self.assertTrue(tasks['T4']['own_test']['passed'], "the arm's own test passes; that is not enough")
        self.assertTrue(tasks['T5']['hidden_test']['passed'])
        self.assertIn('src/format/ was created', tasks['T5']['traps'])
        self.assertIn('src/format/money.ts', result['origin']['outside_scope'])
        self.assertTrue(tasks['T3']['hidden_test']['passed'])
        self.assertEqual(tasks['T3']['traps'], [])
        self.assertEqual(result['accepted'], 0)


if __name__ == '__main__':
    unittest.main()
