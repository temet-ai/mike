import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, MagicMock

from knowledge.collector import scan_root, sync_sources
from knowledge.lessons import parse, render, sync_lessons


class FakeClient:
    def __init__(self, payload=None, version=1):
        self.calls = []
        self.payload = payload
        self.version = version

    def request(self, path, data=None):
        self.calls.append((path, data))
        if path.startswith('/api/knowledge/lessons?'):
            return {'lessons': [{'version': self.version, 'payload': self.payload}]}
        if path.endswith('/ack'):
            return {'acknowledged': True}
        if path == '/api/knowledge/lessons':
            self.payload = data['payload']
            self.version += 1
            return {'version': self.version}
        if path == '/api/knowledge/sources':
            return {'source_uuid': 'source', 'generation': 1}
        return {'generation': 2}


class CollectorTests(unittest.TestCase):
    def test_session_registration_carries_location_and_goal(self):
        from knowledge.cli import main
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.mike').mkdir()
            (root / '.mike/knowledge.json').write_text(json.dumps({'version': 1, 'project_uuid': 'project'}))
            client = MagicMock()
            client.request.side_effect = [{'location_uuid': 'location'}, {}]
            args = ['mike-knowledge', 'session', '--repo', str(root), '--state-dir', str(root / 'state'),
                    '--session-id', 'session', '--goal', 'Investigate knowledge']
            with patch('sys.argv', args), patch('knowledge.cli.Client', return_value=client), patch('builtins.print'):
                main()
            route, payload = client.request.call_args.args
            self.assertEqual(route, '/api/workspace/register')
            self.assertEqual(payload['location_uuid'], 'location')
            self.assertEqual(payload['intent'], 'Investigate knowledge')

    def run_session(self, root, client):
        from knowledge.cli import main
        args = ['mike-knowledge', 'session', '--repo', str(root), '--state-dir', str(root / 'state'),
                '--session-id', 'session']
        with patch('sys.argv', args), patch('knowledge.cli.Client', return_value=client), patch('builtins.print'):
            main()

    def test_session_refreshes_the_location_on_every_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.mike').mkdir()
            (root / '.mike/knowledge.json').write_text(json.dumps({'version': 1, 'project_uuid': 'project'}))
            client = MagicMock()
            client.request.side_effect = [{'location_uuid': 'location'}, {}, {'location_uuid': 'location'}, {}]
            self.run_session(root, client)
            self.run_session(root, client)
            routes = [call.args[0] for call in client.request.call_args_list]
            self.assertEqual(routes, ['/api/knowledge/locations', '/api/workspace/register'] * 2)
            payloads = [call.args[1] for call in client.request.call_args_list]
            self.assertEqual(payloads[1]['location_uuid'], payloads[3]['location_uuid'])
            self.assertEqual(payloads[0]['machine_uuid'], payloads[2]['machine_uuid'])
            self.assertEqual(payloads[0]['root_path'], str(root.resolve()))

    def test_session_location_conflict_never_registers_the_session(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.mike').mkdir()
            (root / '.mike/knowledge.json').write_text(json.dumps({'version': 1, 'project_uuid': 'project'}))
            client = MagicMock()
            client.request.side_effect = RuntimeError('Mike returned 409: Location belongs to another project')
            with self.assertRaises(SystemExit) as raised:
                self.run_session(root, client)
            self.assertEqual(raised.exception.code, 1)
            routes = [call.args[0] for call in client.request.call_args_list]
            self.assertEqual(routes, ['/api/knowledge/locations'])

    def test_machine_identity_is_created_once_in_the_shape_the_rust_reader_expects(self):
        from knowledge.cli import machine_identity
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory) / 'state'
            first = machine_identity(state)
            second = machine_identity(state)
            self.assertEqual(first, second)
            self.assertEqual(sorted(first), ['machine_name', 'machine_uuid'])
            uuid_text = first['machine_uuid']
            self.assertEqual(str(__import__('uuid').UUID(uuid_text)), uuid_text)

    def test_original_bytes_symlink_root_and_generated_exclusion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            actual = root / 'actual'
            actual.mkdir()
            link = root / 'artifacts'
            link.symlink_to(actual, target_is_directory=True)
            (actual / 'knowledge').mkdir()
            (actual / 'knowledge/copy.md').write_text('duplicate')
            body = '---\ntitle: Café\nstatus: proposed\n---\n# Evidence\r\nfull body\n'
            (actual / 'report.md').write_bytes(body.encode())
            docs = scan_root(link, ['knowledge'])
            self.assertEqual(len(docs), 1)
            self.assertEqual(docs[0]['body'], body)
            self.assertEqual(docs[0]['status'], 'proposed')

    def test_missing_source_never_submits_empty_scan(self):
        with tempfile.TemporaryDirectory() as directory:
            client = FakeClient()
            config = {'sources': [{'name': 'artifacts', 'path': 'missing'}]}
            with self.assertRaises(RuntimeError):
                sync_sources(client, Path(directory), config, 'location')
            self.assertFalse(any(path.endswith('/scan') for path, _ in client.calls))

    def test_malformed_frontmatter_is_indexed_with_the_error_not_dropped(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'broken.md').write_text('---\ninvalid: [\n---\nbody')
            (root / 'unclosed.md').write_text('---\ntitle: x\nbody')
            (root / 'good.md').write_text('---\ntitle: Good\n---\nbody')
            docs = {d['relative_path']: d for d in scan_root(root, [])}
            self.assertEqual(set(docs), {'broken.md', 'unclosed.md', 'good.md'})
            self.assertIn('invalid frontmatter', docs['broken.md']['metadata']['frontmatter_error'])
            self.assertEqual(docs['unclosed.md']['metadata']['frontmatter_error'], 'unclosed frontmatter')
            self.assertEqual(docs['broken.md']['body'], '---\ninvalid: [\n---\nbody')
            self.assertEqual(docs['good.md']['title'], 'Good')

    def test_a_failing_source_never_blocks_the_lesson_sync(self):
        from knowledge.cli import sync_once
        with tempfile.TemporaryDirectory() as directory:
            client = MagicMock()
            config = {'sources': [{'name': 'artifacts', 'path': 'missing'}]}
            with patch('knowledge.cli.register', return_value={'location_uuid': 'location'}), \
                    patch('knowledge.cli.sync_lessons', return_value={'lesson_files': 2}) as lessons:
                result = sync_once(client, Path(directory), config, {})
            lessons.assert_called_once()
            self.assertEqual(result['lessons'], {'lesson_files': 2})
            self.assertIn('sources: Source unavailable', result['error'])

    def test_lesson_roundtrip_full_text(self):
        payload = {'lesson_uuid': 'id', 'body': '\n  Keep exact whitespace\n', 'deleted': False}
        self.assertEqual(parse(render(payload, 3)), (payload, 3))

    def test_remote_lifecycle_update_replaces_unmodified_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / '.claude/lessons/id.md'
            path.parent.mkdir(parents=True)
            payload = {'lesson_uuid': 'id', 'body': 'Rule', 'stage': 'raw', 'deleted': False}
            path.write_text(render(payload, 1))
            updated = {**payload, 'stage': 'archived', 'body': 'Rule\r\nWith exact newlines\r\n'}
            client = FakeClient(updated, 2)
            sync_lessons(client, root, 'location')
            self.assertEqual(parse(path.read_bytes().decode('utf-8')), (updated, 2))

    def test_concurrent_local_and_remote_edits_preserve_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / '.claude/lessons/id.md'
            path.parent.mkdir(parents=True)
            payload = {'lesson_uuid': 'id', 'body': 'Rule', 'deleted': False}
            changed = render(payload, 1).replace('\nRule', '\nLocal edit')
            path.write_text(changed)
            client = FakeClient({**payload, 'body': 'Remote edit'}, 2)
            with self.assertRaisesRegex(RuntimeError, 'conflict'):
                sync_lessons(client, root, 'location')
            self.assertEqual(path.read_text(), changed)
            self.assertFalse(any(path.endswith('/ack') for path, _ in client.calls))


if __name__ == '__main__':
    unittest.main()
