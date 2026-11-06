from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compressor_eval import task_trials as tt
from compressor_eval.qualification import qualifies
from compressor_eval.trial_tasks import BY_ID, FAMILIES, TASKS, WEAKENED, build_tree, grade, weaken_section

POLICY = {'implementation': 'file-read-v3-ranged-recovery', 'build_sha': 'abc'}


def fake_capture(session):
    return {'turns': [{'id': 1, 'model': 'sonnet', 'fresh': 10, 'cache_write': 100, 'cache_read': 500, 'output': 20}],
            'calls': [{'id': 1, 'request_id': 3, 'tool_name': 'Read', 'file_path': '/x/docs/guide.md', 'content': 'a', 'created_at': 'a', 'is_error': False},
                      {'id': 2, 'request_id': 10, 'tool_name': 'Read', 'file_path': '/x/docs/guide.md', 'content': 'b', 'created_at': 'b', 'is_error': False}],
            'stages': [{'id': 9, 'outcome': 'applied', 'reason': 'file_read_forwarded', 'arm': 'forward',
                        'original_sha256': 'o', 'sent_sha256': 's'}] if tt.bucket(session) < 20 else []}


LIVE_PROTECTED = ['**/CLAUDE.md', '**/.claude/**', '**/.local/state/mike/**', '**/*.rs', '**/*.py']


class ProtectedDirectory(unittest.TestCase):
    def test_a_plan_directory_under_a_protected_glob_is_refused_with_the_glob_named(self):
        from compressor_eval.protected import refuse_protected_directory
        with self.assertRaises(SystemExit) as caught:
            refuse_protected_directory({'protected_paths': LIVE_PROTECTED}, '/home/u/.local/state/mike/compressor-trials/x')
        self.assertIn('**/.local/state/mike/**', str(caught.exception))

    def test_a_directory_outside_the_protected_globs_is_accepted(self):
        from compressor_eval.protected import refuse_protected_directory
        refuse_protected_directory({'protected_paths': LIVE_PROTECTED}, '/home/u/.local/state/compressor-trials/x')

    def test_glob_semantics_match_the_proxy(self):
        from compressor_eval.protected import matching_glob
        self.assertEqual(matching_glob(['**/CLAUDE.md'], 'CLAUDE.md'), '**/CLAUDE.md')
        self.assertIsNone(matching_glob(['**/BRIEF*.md'], '/a/BRIEF/extra.md'))
        self.assertIsNotNone(matching_glob(['**/*.rs'], '/a/b/c.rs'))


class UncutRun(unittest.TestCase):
    def evidence(self, cuts):
        return {'pairs': [{'compact': {'expected_cut': True, 'cut_requests': c}} for c in cuts]}

    def test_collect_says_a_run_without_cuts_is_not_a_test(self):
        self.assertTrue(tt.verdict_of(self.evidence([0, 0, 0, 1]), POLICY).startswith('not a test of the compressor'))

    def test_a_run_with_cuts_falls_through_to_the_qualification_verdict(self):
        self.assertIn(tt.verdict_of(self.evidence([3, 4, 0, 2]), POLICY), ('qualified', 'not qualified'))


class FixtureGraders(unittest.TestCase):
    def test_every_task_fails_untouched_and_passes_with_its_reference_solution(self):
        for task in TASKS:
            with self.subTest(task.id), tempfile.TemporaryDirectory() as root:
                build_tree(task, root)
                self.assertFalse(grade(task, root)['correct'])
                task.solve(root)
                if task.between:
                    task.between(root)
                self.assertEqual(grade(task, root), {'correct': True, 'failures': [], 'constraint_failures': []})

    def test_compressible_sources_are_large_and_not_protected_code(self):
        for task in TASKS:
            with self.subTest(task.id), tempfile.TemporaryDirectory() as root:
                build_tree(task, root)
                size = (Path(root) / task.source).stat().st_size
                self.assertEqual(size >= 1500, task.expects_cut)
                self.assertNotIn(Path(task.source).suffix, ('.py', '.sh', '.rs'))

    def test_overreaching_edit_fails_its_constraint(self):
        with tempfile.TemporaryDirectory() as root:
            task = BY_ID['doc-flag']
            build_tree(task, root)
            path = Path(root) / task.source
            path.write_text(path.read_text().replace('--retry-limit', '--max-retries'))
            self.assertTrue(grade(task, root)['constraint_failures'] or grade(task, root)['failures'])

    def test_each_family_has_three_tasks_and_all_four_controls_exist(self):
        for family in FAMILIES:
            self.assertEqual(sum(t.family == family for t in TASKS), 3)
        self.assertEqual(sum(t.family.startswith('control-') for t in TASKS), 4)

    def test_weakened_tree_cannot_pass_from_disk(self):
        with tempfile.TemporaryDirectory() as root:
            build_tree(WEAKENED, root)
            weaken_section(root)
            self.assertNotIn('37', (Path(root) / WEAKENED.source).read_text().partition('## Backups')[2].split('\n')[1])


class Recovery(unittest.TestCase):
    def test_only_reads_of_the_source_from_the_cut_onward_count(self):
        capture = fake_capture('compact')
        capture['stages'] = [{'id': 9, 'outcome': 'applied'}]
        self.assertEqual(tt.recoveries(capture, 'docs/guide.md'), 1)
        self.assertEqual(tt.recoveries(capture, 'docs/other.md'), 0)
        capture['stages'] = [{'id': 9, 'outcome': 'unchanged'}]
        self.assertEqual(tt.recoveries(capture, 'docs/guide.md'), 0)


class Plan(unittest.TestCase):
    def test_session_ids_select_each_arm_and_order_varies(self):
        plan = tt.make_plan('sonnet', 'medium', '2.1', 20, 3, 11)
        self.assertEqual(len(plan['runs']), 13 * 3 * 2 + 1)
        for run in plan['runs']:
            self.assertEqual(tt.bucket(run['session']) < 20, run['arm'] == 'compact')
        firsts = {r['arm'] for r in plan['runs'][::2][:39]}
        self.assertEqual(firsts, {'original', 'compact'})
        self.assertEqual(plan, tt.make_plan('sonnet', 'medium', '2.1', 20, 3, 11))

    def test_zero_share_is_refused(self):
        with self.assertRaises(ValueError):
            tt.make_plan('sonnet', 'medium', '2.1', 0, 3, 11)


class DryRun(unittest.TestCase):
    def setUp(self):
        self.plan = tt.make_plan('sonnet', 'medium', '2.1', 20, 3, 11)
        self.plan['cold_wait_seconds'] = 0

    def execute(self, directory, agent):
        tt.run_all(self.plan, directory, agent, sleep=lambda _s: None)
        return tt.collect(self.plan, directory, fake_capture, POLICY, kind='dry_run')

    def test_dry_run_scores_every_pair_and_can_never_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = self.execute(directory, tt.ScriptedAgent())
            self.assertEqual(len(evidence['pairs']), 39)
            self.assertEqual(evidence['runs_finished'], 79)
            self.assertTrue(evidence['recovery']['weakened_control_failed'])
            self.assertEqual(evidence['summary']['tasks']['doc-port']['correct_compact'], 3)
            self.assertFalse(qualifies(evidence, POLICY))

    def test_a_wrong_answer_lowers_correctness_and_a_rerun_resumes(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = self.execute(directory, tt.ScriptedAgent(wrong=('log-real',)))
            self.assertEqual(evidence['summary']['tasks']['log-real']['correct_original'], 0)
            self.assertIsNone(evidence['summary']['tasks']['log-real']['relative_cost_change'])
            again = self.execute(directory, tt.ScriptedAgent())
            self.assertEqual(again['summary']['tasks']['log-real']['correct_original'], 0)

    def test_actual_kind_with_all_correct_runs_can_qualify(self):
        with tempfile.TemporaryDirectory() as directory:
            tt.run_all(self.plan, directory, tt.ScriptedAgent(), sleep=lambda _s: None)
            evidence = tt.collect(self.plan, directory, fake_capture, POLICY)
            self.assertTrue(qualifies(evidence, POLICY))


if __name__ == '__main__':
    unittest.main()
