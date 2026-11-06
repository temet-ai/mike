import concurrent.futures
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compressor_eval import review_queue as queue


def nomination(kind='tool_result', request=473363, tool=808536):
    return {'target': {'kind': kind, 'request_id': request, 'tool_id': tool if kind == 'tool_result' else None},
            'reason': 'Failure diagnostics needed a second call', 'evidence': 'Result request 473366'}


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.state = Path(self.temp.name) / 'queue'

    def test_target_kinds_have_separate_identity(self):
        rows = queue.flag_many(self.state, [nomination(k) for k in sorted(queue.KINDS)], 'test')
        self.assertEqual(len({r['id'] for r in rows}), 3)
        self.assertEqual(len(queue.read(self.state)), 3)

    def test_duplicate_nomination_is_idempotent(self):
        first = queue.flag_many(self.state, [nomination()], 'test')
        second = queue.flag_many(self.state, [nomination()], 'test')
        self.assertEqual(first[0]['id'], second[0]['id'])
        self.assertFalse(second[0]['added'])
        self.assertEqual(len(queue.read(self.state)[first[0]['id']]['history']), 1)

    def test_review_preserves_history_and_requires_explicit_reopen(self):
        key = queue.flag_many(self.state, [nomination()], 'test')[0]['id']
        queue.review(self.state, key, 'dismissed', 'Already task-specific', 'reviewer')
        changed = nomination()
        changed['reason'] = 'Additional evidence'
        queue.flag_many(self.state, [changed], 'test')
        item = queue.read(self.state)[key]
        self.assertEqual(item['status'], 'dismissed')
        self.assertEqual(len(item['history']), 3)
        queue.review(self.state, key, 'open', 'Revisit new evidence', 'reviewer')
        self.assertEqual(queue.read(self.state)[key]['status'], 'open')

    def test_invalid_batch_writes_nothing(self):
        with self.assertRaises(ValueError):
            queue.flag_many(self.state, [nomination(), nomination(tool=None)], 'test')
        self.assertFalse(self.state.exists())

    def test_unknown_review_rejected(self):
        with self.assertRaises(ValueError):
            queue.review(self.state, 'absent', 'evaluated', 'Evidence', 'reviewer')

    def test_rust_nomination_without_python_hash_deduplicates(self):
        item = nomination()
        value = queue.target(**item['target'])
        with queue.locked(self.state) as (file, _):
            queue.append(file, {'action': 'flag', 'id': queue.identity(value), 'target': value,
                                'reason': item['reason'], 'evidence': item['evidence'], 'actor': 'inspect-ui'})
        result = queue.flag_many(self.state, [item], 'inspect-ui')
        self.assertFalse(result[0]['added'])

    def test_private_files(self):
        queue.flag_many(self.state, [nomination()], 'test')
        self.assertEqual(self.state.stat().st_mode & 0o777, 0o700)
        self.assertEqual((self.state / 'events.jsonl').stat().st_mode & 0o777, 0o600)

    def test_concurrent_duplicates_have_one_event(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
            results = list(pool.map(lambda _: queue.flag_many(self.state, [nomination()], 'test'), range(20)))
        self.assertEqual(sum(r[0]['added'] for r in results), 1)
        self.assertEqual(len((self.state / 'events.jsonl').read_text().splitlines()), 1)

    def test_import_uses_result_receipt_not_emitting_request(self):
        report = {'version': 1, 'edges': [{'source_result_request_id': 473363, 'source_tool_id': 808536,
                  'followup_tool_id': 808542, 'followup_request_id': 473363, 'operations': ['filter']}]}
        records = queue.nominations(report, 'sha256:test')
        self.assertEqual(records[0]['target']['request_id'], 473363)
        self.assertIn('unverified', records[0]['evidence'])

    def test_original_response_link(self):
        key = queue.flag_many(self.state, [nomination('response')], 'test')[0]['id']
        self.assertEqual(queue.links(queue.read(self.state)[key])['original_body'], '/api/requests/473363/body/response')

    def test_cli_flag_show_and_review(self):
        command = [sys.executable, str(Path(__file__).resolve().parents[1] / 'compression-review'), '--state', str(self.state)]
        first = subprocess.run(command + ['flag', 'request', '473363', '--reason', 'Repeated instructions',
                               '--evidence', 'manual review', '--actor', 'test'], capture_output=True, text=True, check=True)
        key = json.loads(first.stdout)[0]['id']
        shown = subprocess.run(command + ['show', key], capture_output=True, text=True, check=True)
        self.assertFalse(json.loads(shown.stdout)['capture_verified'])
        subprocess.run(command + ['review', key, 'investigating', '--note', 'Review started', '--actor', 'test'],
                       capture_output=True, check=True)
        listing = subprocess.run(command + ['list', '--status', 'investigating'], capture_output=True, text=True, check=True)
        self.assertEqual(len(json.loads(listing.stdout)), 1)


if __name__ == '__main__':
    unittest.main()
