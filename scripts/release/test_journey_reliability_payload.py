"""Offline payload/tar/evidence contracts; no binary/container/database execution."""
import copy
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
from unittest.mock import patch

import journey_reliability_evidence as evidence
import journey_reliability_payload as payload
from journey import new_result
from journey_process import JourneyError
from test_journey_reliability import OfflineTests


def candidate(root):
    manifest = {'version': '0.0.1', 'source_commit': 'a' * 40, 'target': 'x86_64-unknown-linux-gnu', 'artifacts': []}
    for name in ('mike', 'mike-mcp'):
        filename = f'{name}-v0.0.1-x86_64-unknown-linux-gnu.tar.gz'
        raw = ('synthetic archive bytes for ' + name).encode()
        digest = hashlib.sha256(raw).hexdigest()
        (root / filename).write_bytes(raw)
        (root / (filename + '.sha256')).write_text(digest + '  ' + filename + '\n')
        manifest['artifacts'].append({'binary': name, 'archive': filename, 'sha256': digest,
                                     'contents': [{'path': name, 'sha256': 'b' * 64, 'size': 5}]})
    data = json.dumps(manifest, sort_keys=True).encode()
    (root / 'manifest.json').write_bytes(data)
    return manifest, hashlib.sha256(data).hexdigest()


def tar_result(value, *, name='result.json', duplicate=False, link=False):
    raw = json.dumps(value).encode()
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w') as archive:
        info = tarfile.TarInfo(name)
        info.size = len(raw)
        if link:
            info.type, info.linkname, info.size = tarfile.SYMTYPE, '/private', 0
        archive.addfile(info, io.BytesIO(raw))
        if duplicate:
            archive.addfile(info, io.BytesIO(raw))
    return output.getvalue()


class PayloadTests(OfflineTests):
    def test_tar_uses_fixed_paths_modes_ownership_and_bound_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); snapshot = root / 'candidate'; snapshot.mkdir()
            manifest, digest = candidate(snapshot)
            source = root / 'tool.py'; source.write_text('value = 1\n')
            destination = root / 'input.tar'
            with patch.object(payload, 'source_closure', return_value=[source]):
                result = payload.prepare_payload(snapshot, manifest, digest, destination, {'duration_seconds': 900},
                                                 host_namespaces={'net': 'net:[1]'})
            with tarfile.open(destination) as archive:
                members = archive.getmembers()
                self.assertTrue(all(member.uid == payload.UID and member.gid == payload.GID for member in members))
                self.assertTrue(all(member.mode == (0o700 if member.isdir() else 0o600) for member in members))
                self.assertTrue(all(not member.issym() and not member.islnk() for member in members))
                self.assertEqual(members[-1].name, 'payload/launch.json')
                raw = archive.extractfile(members[-1]).read()
                self.assertEqual(hashlib.sha256(raw).hexdigest(), result['launch_sha256'])
                launch = json.loads(raw)
                for record in launch['files']:
                    raw = archive.extractfile('payload/' + record['path']).read()
                    self.assertEqual(hashlib.sha256(raw).hexdigest(), record['sha256'])
                    self.assertEqual(len(raw), record['size'])
            self.assertEqual(destination.stat().st_mode & 0o777, 0o600)

    def test_wrong_digest_extra_files_symlink_and_expanded_overflow_rejected(self):
        for mode in ('digest', 'extra', 'symlink', 'expanded'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); snapshot = root / 'candidate'; snapshot.mkdir()
                manifest, digest = candidate(snapshot)
                first = snapshot / manifest['artifacts'][0]['archive']
                if mode == 'digest':
                    first.write_bytes(b'changed')
                elif mode == 'extra':
                    (snapshot / 'private.txt').write_text('private')
                elif mode == 'symlink':
                    first.unlink(); first.symlink_to('/dev/null')
                else:
                    manifest['artifacts'][0]['contents'][0]['size'] = payload.MAX_PAYLOAD_BYTES + 1
                    data = json.dumps(manifest).encode(); (snapshot / 'manifest.json').write_bytes(data)
                    digest = hashlib.sha256(data).hexdigest()
                with patch.object(payload, 'source_closure', return_value=[]), self.assertRaises(JourneyError):
                    payload.prepare_payload(snapshot, manifest, digest, root / 'input.tar', {}, host_namespaces={})

    def test_pinned_stream_detects_growth_shrink_and_changed_tar_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input'; path.write_bytes(b'first')
            with payload.bounded_file(path, 100)[0] as source:
                digest = payload.stream_digest(source, 5)
                reader = payload.CheckedReader(source, 5)
                path.write_bytes(b'other')
                self.assertEqual(reader.read(5), b'other')
                with self.assertRaisesRegex(JourneyError, 'PAYLOAD_CHANGED'):
                    reader.finish(digest)
            for raw in (b'', b'longer'):
                path.write_bytes(b'first')
                with payload.bounded_file(path, 100)[0] as source:
                    path.write_bytes(raw)
                    with self.assertRaisesRegex(JourneyError, 'PAYLOAD_CHANGED'):
                        payload.stream_digest(source, 5)

    def test_aggregate_input_limit_rejected_before_creating_tar(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); snapshot = root / 'candidate'; snapshot.mkdir()
            manifest, digest = candidate(snapshot)
            source = root / 'large.py'; source.write_bytes(b'x' * 2048)
            destination = root / 'input.tar'
            with patch.object(payload, 'source_closure', return_value=[source]), patch.object(payload, 'MAX_PAYLOAD_BYTES', 1800), \
                    self.assertRaisesRegex(JourneyError, 'PAYLOAD_(?:TOTAL|FILE)_LIMIT'):
                payload.prepare_payload(snapshot, manifest, digest, destination, {}, host_namespaces={})
            self.assertFalse(destination.exists())

    def test_safe_tooling_closure_is_source_only_and_includes_inner_modules(self):
        root = Path(payload.__file__).parent
        paths = payload.source_closure(root)
        self.assertIn(root / 'journey_reliability_container.py', paths)
        self.assertIn(root / 'journey_reliability_normal.py', paths)
        self.assertTrue(all(path.parent == root and path.suffix == '.py' and not path.name.startswith('test_') for path in paths))

    def test_result_tar_never_extracts_paths_links_or_extra_members(self):
        value = {'state': 'FAILED'}
        self.assertEqual(payload.result_from_tar(tar_result(value))[0], value)
        for options in ({'name': '../result.json'}, {'duplicate': True}, {'link': True}):
            with self.subTest(options=options), self.assertRaises(JourneyError):
                payload.result_from_tar(tar_result(value, **options))
        with self.assertRaises(JourneyError):
            payload.result_from_tar(b'x' * (payload.MAX_RESULT_BYTES + 65537))


class EvidenceTests(OfflineTests):
    def test_projection_drops_logs_paths_urls_and_arbitrary_prose(self):
        value = {'samples': 3, 'state': 'PASSED', 'raw_stdout': 'secret', 'path': '/home/private',
                 'observation': {'rss_current_bytes': 10, 'description': 'private'},
                 'worker_exercise': {'harvester': 'PASSED', 'embedding_consolidation': 'NOT_RUN'},
                 'boot_defaults': {'compiled_boot_defaults_preserved': True, 'boot_snapshot_sha256': 'a' * 64},
                 'worker_observations': {'registrations': 2, 'exact_registration_and_lesson_provenance': True}}
        result = evidence.projection(value)
        self.assertNotIn('secret', json.dumps(result)); self.assertNotIn('private', json.dumps(result))
        self.assertEqual(result['worker_exercise']['harvester'], 'PASSED')
        self.assertEqual(result['worker_observations']['registrations'], 2)
        for field, item in (('samples', float('nan')), ('samples', True), ('exact_installed_hashes', 'true'),
                            ('capture_set_sha256', 'private'), ('workload_state', 'qualified')):
            with self.subTest(field=field), self.assertRaises(JourneyError):
                evidence.projection({field: item})

    def test_binding_qualification_cases_and_profile_are_independently_validated(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest, digest = candidate(Path(directory))
        original = new_result(evidence.expected_binding(manifest, digest))
        original['checks'] = [{'id': 'archive_normal_default_workload', 'state': 'PASSED',
                               'observation': {'samples': 1}}]
        original['reliability'] = {'profile': 'normal_defaults', 'workload_state': 'PASSED'}
        self.assertFalse(evidence.sanitize_result(original, manifest, digest)['qualified'])
        changes = [({'qualified': True}), ({'binding': {}}), ({'state': 'PASSED'}),
                   ({'full_cases': []}), ({'reliability': {'profile': 'synthetic_isolation_overrides'}})]
        for change in changes:
            value = dict(original, **change)
            with self.subTest(change=change), self.assertRaises(JourneyError):
                evidence.sanitize_result(value, manifest, digest)
