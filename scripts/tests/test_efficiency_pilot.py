import importlib.machinery
import importlib.util
from pathlib import Path
import sys
import unittest
import json
import tempfile
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from efficiency_pilot_tasks import grade, task_files, write_files
from efficiency_pilot_scenarios import SCENARIOS
loader = importlib.machinery.SourceFileLoader('efficiency_pilot', str(Path(__file__).resolve().parents[1] / 'efficiency-pilot'))
spec = importlib.util.spec_from_loader(loader.name, loader)
pilot = importlib.util.module_from_spec(spec)
loader.exec_module(pilot)


class PilotEvidence(unittest.TestCase):
    def test_incomplete_output_is_not_success_or_zero_usage(self):
        value = pilot.metrics('provider failed\n')
        self.assertFalse(value['model_completed'])
        self.assertIsNone(value['actual_total_tokens'])
        self.assertEqual(value['malformed_event_lines'], 1)

    def test_final_usage_includes_cached_tokens_and_calls_are_deduplicated(self):
        call = dict(type='assistant', message=dict(content=[dict(type='tool_use', id='a')]))
        end = dict(type='result', usage=dict(input_tokens=1, output_tokens=2,
                   cache_creation_input_tokens=3, cache_read_input_tokens=4), num_turns=2)
        value = pilot.metrics('\n'.join(json.dumps(e) for e in (call, call, end)))
        self.assertEqual(value['actual_total_tokens'], 10)
        self.assertEqual(value['tool_calls'], 1)
        self.assertTrue(value['model_completed'])


class RealisticGraders(unittest.TestCase):
    def test_all_broken_controls_fail_and_reference_solutions_pass(self):
        for name, task in SCENARIOS.items():
            with self.subTest(task=name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                workspace = root / 'work'
                workspace.mkdir()
                for label, solved in [('broken', False), ('correct', True)]:
                    write_files(workspace, task_files(task, solved))
                    self.assertEqual(grade(task, workspace, root / label)['correct'], solved)

    def test_changing_contract_or_pointing_outside_workspace_does_not_pass(self):
        task = SCENARIOS['python-config']
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workspace = root / 'work'
            workspace.mkdir()
            write_files(workspace, task_files(task, True))
            (workspace / 'data/defaults.json').write_text('{"retries": 0, "trace": false}')
            self.assertFalse(grade(task, workspace, root / 'tampered')['correct'])
            write_files(workspace, task_files(task, True))
            candidate = workspace / task['file']
            external = root / 'external.py'
            external.write_text(candidate.read_text())
            candidate.unlink()
            candidate.symlink_to(external)
            self.assertFalse(grade(task, workspace, root / 'symlink')['correct'])


if __name__ == '__main__':
    unittest.main()
