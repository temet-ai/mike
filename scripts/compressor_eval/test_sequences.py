import copy
from pathlib import Path
import sys
import json
import subprocess
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compressor_eval.commands import inspect
from compressor_eval.core import analyze, text_bytes
from compressor_eval.trials import compare


def call(identity, command='cat /tmp/run.log', second=0, agent='a'):
    return {'id': identity, 'tool_name': 'Bash', 'input': {'command': command},
            'request_id': identity, 'result_request_id': identity + 100,
            'session_id': 1, 'agent_instance_id': agent, 'claude_code_agent_id': None,
            'operator_turn_id': 1, 'created_at': f'2026-10-04T00:00:{second:02d}Z',
            'result_at': f'2026-10-04T00:00:{second+1:02d}Z'}


def result(c):
    return {'id': c['id'], 'result_request_id': c['result_request_id'], 'content': 'é'}


class Sequences(unittest.TestCase):
    def test_shell_variable_and_mixed_call(self):
        paths, kinds = inspect(call(1, "L=/tmp/run.log; grep -A3 panicked $L | head -12; git diff --stat"))
        self.assertEqual(paths, {'/tmp/run.log'})
        self.assertEqual(kinds, {'filter', 'slice', 'other_or_mixed'})

    def test_heredoc_is_not_executed_reader(self):
        self.assertEqual(inspect(call(1, "cat > /tmp/x <<'EOF'\ngrep /tmp/run.log\nEOF")),
                         (set(), {'unparsed_shell'}))

    def test_git_path_is_not_a_read(self):
        self.assertEqual(inspect(call(1, 'git add /tmp/run.log'))[0], set())

    def test_same_lane_and_after_receipt(self):
        a, parallel, other, good = call(1), call(2), call(3, second=2, agent='b'), call(4, second=3)
        report = analyze([a, parallel, other, good], [result(a)])
        self.assertEqual([e['followup_tool_id'] for e in report['edges']], [4])

    def test_equal_receipt_time_is_allowed(self):
        a, b = call(1), call(2, second=1)
        self.assertEqual(len(analyze([a, b], [result(a)])['edges']), 1)

    def test_window_excludes_late_calls(self):
        a, b = call(1), call(2, second=8)
        self.assertEqual(analyze([a, b], [result(a)], window=2)['edges'], [])

    def test_overlapping_edges_count_each_call_once(self):
        calls = [call(i, second=i*2) for i in range(1, 4)]
        report = analyze(calls, [result(c) for c in calls])
        self.assertEqual(report['counts']['candidate_edges'], 3)
        self.assertEqual(report['counts']['unique_calls_in_edges'], 3)
        self.assertEqual(report['counts']['known_result_text_bytes_once_per_call'], 6)

    def test_missing_result_not_zero_cost(self):
        a, b = call(1), call(2, second=2)
        report = analyze([a, b], [result(a)])
        self.assertEqual(report['counts']['calls_missing_decoded_result'], 1)
        self.assertIsNone(report['costs']['actual_model_tokens'])

    def test_images_excluded(self):
        self.assertEqual(text_bytes([{'type': 'image', 'source': {'data': 'abc'*100}},
                                     {'type': 'text', 'text': 'é'}]), 2)

    def test_conflicting_pairing_rejected(self):
        a = call(1)
        with self.assertRaises(ValueError):
            analyze([a], [{**result(a), 'result_request_id': 999}])

    def test_conflicting_duplicate_rejected(self):
        with self.assertRaises(ValueError):
            analyze([call(1), call(1, second=2)], [])

    def test_unrelated_label_rejected(self):
        with self.assertRaises(ValueError):
            analyze([], [], [{'source_tool_id': 1, 'followup_tool_id': 2}])

    def test_review_and_missing_agent(self):
        a, b = call(1, agent=None), call(2, second=2, agent=None)
        label = {'source_tool_id': 1, 'followup_tool_id': 2, 'purposes': ['mixed'], 'evidence': 'inspected'}
        report = analyze([a, b], [result(a)], [label])
        self.assertEqual(report['counts']['reviewed_edges'], 1)
        self.assertFalse(report['edges'][0]['agent_identity_known'])


    def test_fixed_sources_do_not_expand_with_retrieved_results(self):
        calls = [call(i, second=i*2) for i in range(1, 4)]
        report = analyze(calls, [result(c) for c in calls], source_ids=[1])
        self.assertEqual(report['counts']['source_results'], 1)
        self.assertEqual(report['counts']['candidate_edges'], 2)
        self.assertEqual(report['counts']['calls_missing_decoded_result'], 0)

    def test_cli_reproducible_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = [call(1), call(2, second=2)]
            (root / 'calls.json').write_text(json.dumps(calls))
            (root / 'results.json').write_text(json.dumps([result(c) for c in calls]))
            command = [sys.executable, str(Path(__file__).resolve().parents[1] / 'compressor-sequence-eval'),
                       '--calls', str(root / 'calls.json'), '--results', str(root / 'results.json')]
            for name in ('first', 'second'):
                completed = subprocess.run(command + ['--out', str(root / name)], capture_output=True)
                self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual((root / 'first/report.json').read_bytes(), (root / 'second/report.json').read_bytes())
            completed = subprocess.run(command + ['--out', str(root / 'first')], capture_output=True)
            self.assertEqual(completed.returncode, 2)
            self.assertEqual((root / 'first').stat().st_mode & 0o777, 0o700)
            self.assertNotIn('/tmp/run.log', (root / 'first/report.json').read_text())

class Trials(unittest.TestCase):
    def pair(self):
        arm = {'execution_kind': 'actual', 'execution_ref': 'fixture-only', 'correct': True,
               'verdict_ref': 'fixture-verdict', 'task_hash': 't', 'ground_truth_hash': 'g',
               'artifact_hash': 'a', 'model_config_hash': 'm',
               'started_at': '2026-10-04T00:00:00Z', 'finished_at': '2026-10-04T00:00:05Z',
               'calls': [{'id': 1, 'content': 'x'}], 'model_turns': [{'id': 1, 'input_tokens': 100, 'output_tokens': 10}],
               'required_facts': [{'id': 'f', 'first_call_id': 1}]}
        return {'task_id': 'synthetic-unit-test', 'original': copy.deepcopy(arm), 'compact': copy.deepcopy(arm)}

    def test_wrong_answer_is_not_success(self):
        pair = self.pair()
        pair['compact']['correct'] = False
        self.assertFalse(compare([pair])[0]['both_correct'])

    def test_retrieval_cost_included(self):
        pair = self.pair()
        pair['compact']['calls'].append({'id': 2, 'content': 'retrieval'})
        pair['compact']['model_turns'].append({'id': 2, 'input_tokens': 200, 'output_tokens': 20})
        row = compare([pair])[0]
        self.assertEqual(row['call_delta_compact_minus_original'], 1)
        self.assertEqual(row['token_delta_compact_minus_original'], 220)

    def test_static_claim_rejected(self):
        pair = self.pair()
        pair['compact']['execution_kind'] = 'static'
        with self.assertRaises(ValueError):
            compare([pair])

    def test_different_ground_truth_rejected(self):
        pair = self.pair()
        pair['compact']['ground_truth_hash'] = 'different'
        with self.assertRaises(ValueError):
            compare([pair])

    def test_missing_usage_is_unknown(self):
        pair = self.pair()
        pair['compact']['model_turns'][0].pop('input_tokens')
        self.assertIsNone(compare([pair])[0]['token_delta_compact_minus_original'])


if __name__ == '__main__':
    unittest.main()
