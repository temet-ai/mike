import sys
from pathlib import Path
import unittest
import tempfile
import json

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from efficiency_report.core import summarize
from efficiency_report.receipts import timing_total
from efficiency_report.prompts import measure


class EfficiencyReport(unittest.TestCase):
    def status(self):
        return dict(fleet='f', run_id='r', state='done', started_at='2026-10-01T00:00:00Z',
                    finished_at='2026-10-01T00:01:00Z',
                    tasks=[dict(id='a', state='passed', criteria=['one'])], obligations=[])

    def test_missing_usage_and_old_measure_are_unknown(self):
        row = summarize('project', self.status(), {}, None, 1)
        self.assertIsNone(row['tokens_per_accepted_task'])
        self.assertIsNone(row['operation_repeats'])
        self.assertEqual(row['elapsed_seconds_per_accepted_task'], 60)

    def test_complete_usage_and_required_obligations_control_denominator(self):
        status = self.status()
        usage = dict(requests=3, complete_requests=3, resolved_sessions=1, observed_tokens=120)
        row = summarize('project', status, {}, usage, 1)
        self.assertEqual(row['tokens_per_accepted_task'], 120)
        status['obligations'] = [dict(required=True, tasks=['a'], criteria=[], outcome='pending')]
        row = summarize('project', status, {}, usage, 1)
        self.assertEqual(row['accepted_tasks'], 0)
        self.assertIsNone(row['tokens_per_accepted_task'])

    def test_open_runs_and_partial_usage_cannot_look_cheap(self):
        status = self.status()
        status['state'] = 'running'
        row = summarize('project', status, {}, dict(requests=3, complete_requests=2,
                        resolved_sessions=1, observed_tokens=90), 1)
        self.assertIsNone(row['tokens_per_accepted_task'])
        self.assertIsNone(row['elapsed_seconds_per_accepted_task'])
        self.assertIn('open_run_right_censored', row['gaps'])


    def test_timing_joins_only_matching_finished_receipts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / '1.json').write_text(json.dumps(dict(schema='operation-timing-v1', receipt=1,
                 ledger='run', queue_and_setup_ms=500, execution_and_cleanup_ms=250)))
            rows = [dict(id=1, run_id='run', finished_at='now'), dict(id=2, run_id='run', finished_at=None)]
            result = timing_total(rows, root)
            self.assertEqual(result['observed_queue_and_setup_seconds'], 0.5)
            self.assertFalse(result['timing_complete'])
            rows[0]['run_id'] = 'other'
            self.assertIsNone(timing_total(rows, root)['observed_queue_and_setup_seconds'])

    def test_prompt_audit_exports_hashes_not_captured_text(self):
        instruction = 'private instruction ' * 8
        tool = dict(name='Read', input_schema=dict(type='object'))
        report = measure(dict(system=instruction, tools=[tool, tool], messages=[]),
                         {instruction.strip(): {'CLAUDE.md'}})
        self.assertEqual(report['duplicated_tool_definitions'], 1)
        self.assertNotIn('private instruction', json.dumps(report))
        self.assertEqual(report['source_matches'][0]['source_paths'], ['CLAUDE.md'])


if __name__ == '__main__':
    unittest.main()
