"""Offline adapter orchestration; fake daemon only, with sockets/processes denied."""
from contextlib import contextmanager
import copy
import json
from pathlib import Path
from unittest.mock import Mock, patch

import journey_reliability_runner as runner
import journey_reliability_enforcement as enforcement
from journey import new_result
from journey_process import JourneyError
from journey_reliability_evidence import expected_binding
from journey_reliability import new_reliability_report, reliability_policy
from journey_reliability_profiles import NORMAL_ENABLED
from journey_reliability_isolation import FEATURES
from journey_reliability_defaults import ENDPOINT_OVERRIDES
from test_journey_reliability import OfflineTests
from test_journey_reliability_enforcement import daemon
from test_journey_reliability_payload import tar_result
from test_journey_reliability_runner_contract import CID, IMAGE, IMAGE_ID, NAME, OWNER, container, image

BUDGETS = {'rss_peak_bytes': 1024 ** 3, 'fd_peak': 1000, 'broadcast_backlog_peak': 200,
           'active_streams_peak': 8, 'drain_seconds': 10, 'responsiveness_seconds': 10}
MANIFEST = {'source_commit': 'a' * 40, 'version': '0.0.1', 'target': 'x86_64-unknown-linux-gnu',
            'artifacts': [{'binary': name, 'sha256': 'b' * 64,
                           'contents': [{'path': name, 'sha256': 'c' * 64}]}
                          for name in ('mike', 'mike-mcp')]}
DIGEST = 'd' * 64


def inner_report():
    result = new_result(expected_binding(MANIFEST, DIGEST))
    result['reliability'] = new_reliability_report(reliability_policy(), profile='normal_defaults', resource_budgets=BUDGETS)
    result['reliability'].update({'profile': 'normal_defaults', 'workload_state': 'PASSED',
                           'duration_policy_met': True, 'concurrency_policy_met': True, 'duration_seconds': 901,
                           'minimum_batch_concurrency': 1, 'candidate_effective_defaults_verified': True,
                           'effective_feature_flags': {key: key in NORMAL_ENABLED for key in FEATURES},
                           'override_names': list(ENDPOINT_OVERRIDES), 'attempted_requests': 91,
                           'completed_client_requests': 91, 'durably_accounted_requests': 91, 'completed_batches': 91,
                           'resources': {'state': 'PASSED', 'samples': 1},
                           'product_observations': {'state': 'PASSED', 'samples': 1},
                           'responsiveness': {'state': 'PASSED', 'samples': 1},
                           'worker_observations': {'registrations': 8, 'harvested_lessons': 2888,
                                                  'exact_registration_and_lesson_provenance': True},
                           'worker_exercise': {'harvester': 'PASSED', 'workspace_capture_registration': 'PASSED',
                                               'background_worker_internal_cycles': 'NOT_RUN'}})
    result['checks'] = [{'id': 'archive_normal_default_workload', 'state': 'PASSED',
                         'observation': {'requests': 2888, 'private': 'do not export'}}]
    return result


class FakeDocker:
    def __init__(self):
        self.verified_peer = {'executable': {'sha256': 'f' * 64}}
        self.seccomp_profile = Path("/offline/normal-default-seccomp.json")
        self.calls = []
        self.created, self.running = False, False
        self.create_command = None
        self.fail_on = None
        self.foreign_cleanup = False
        self.report = inner_report()
        self.inspections = 0

    def command(self, args, **options):
        self.calls.append(list(args))
        step = args[1] if args[0] == 'container' else args[0]
        if step == 'create':
            self.created = True
            self.create_command = args[-5:]
        if self.fail_on == step:
            raise JourneyError('ISOLATED_DOCKER_COMMAND_FAILED')
        if step == 'info':
            return json.dumps(daemon()).encode()
        if step == 'create':
            return (CID + '\n').encode()
        if step == 'start':
            self.running = True
            return (CID + '\n').encode()
        if step == 'cp':
            if args[-1] == '-':
                return tar_result(self.report)
            self.assert_input = options['input_file'].read()
            return b''
        if step == 'exec':
            return b'ready\n'
        if step == 'rm':
            return (CID + '\n').encode()
        raise AssertionError('unexpected fake operation')

    def inspect(self, kind, identity):
        self.calls.append([kind, 'inspect', identity])
        if kind == 'image':
            return image()
        self.inspections += 1
        value = container(self.running)
        value['Config']['Cmd'] = self.create_command
        if self.foreign_cleanup and (self.fail_on is not None or self.inspections > 3):
            value['Config']['Labels'][runner.owns_container.__globals__['OWNER_LABEL']] = 'foreign'
        return value


class RunnerTests(OfflineTests):
    @contextmanager
    def fake(self, docker=None, *, enforcement_error=False):
        docker = docker or FakeDocker()
        @contextmanager
        def context(*_):
            yield docker
        def prepare(_snapshot, _manifest, _digest, destination, _request):
            destination.write_bytes(b'offline private bounded tar')
            return {'launch_sha256': 'e' * 64, 'transfer_sha256': 'f' * 64, 'file_count': 2, 'bytes': 20}
        observation = {'host_process_boundary_observed': True, 'all_transport_isolation_verified': True,
                       'cgroup_enforcement_observed': True, 'host_process_id': 42, 'process_start_ticks': 12}
        with patch.object(enforcement, 'verify_runtime', return_value={'sha256': 'a' * 64}), \
                patch.object(runner, 'local_docker', context), patch.object(runner, 'prepare_payload', side_effect=prepare), \
                patch.object(runner.uuid, 'uuid4', return_value=Mock(hex=OWNER)), \
                patch.object(runner, 'live_container_contract', side_effect=JourneyError('ISOLATED_HOST_ENFORCEMENT_UNAVAILABLE')
                             if enforcement_error else None, return_value=observation) as live:
            yield docker, live

    def run_fake(self, **options):
        return runner.run_isolated_normal_journey(Path('/offline/candidate'), MANIFEST, DIGEST,
            resource_budgets=BUDGETS, docker_binary=Path('/usr/bin/docker'),
            docker_endpoint=Path('/run/docker.sock'), runner_image=IMAGE, runtime_name='mike-release-native', **options)

    def test_complete_protocol_checks_host_enforcement_before_copy_and_cleans_owned_id(self):
        with self.fake() as (docker, live):
            result = self.run_fake()
        self.assertEqual(result['state'], 'INCOMPLETE')
        self.assertEqual(result['isolation']['state'], 'PASSED')
        self.assertTrue(result['isolation']['all_transport_isolation_verified'])
        self.assertEqual(live.call_count, 2)
        self.assertEqual(result['container_cleanup']['state'], 'PASSED')
        self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])
        self.assertNotIn('private', json.dumps(result))
        self.assertTrue(all(case['state'] == 'NOT_RUN' for case in result['full_cases']))
        self.assertEqual(result['reliability']['worker_observations']['registrations'], 8)
        self.assertFalse(result['qualified'])
        self.assertEqual(docker.assert_input, b'offline private bounded tar')
        self.assertNotIn('stage', result['reliability'])
        self.assertEqual(result['requested_policy']['resource_budgets'], BUDGETS)

    def test_failed_actual_enforcement_never_copies_payload_or_claims_admission(self):
        with self.fake(enforcement_error=True) as (docker, _):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['isolation']['state'], 'FAILED')
        self.assertFalse(any(call[:2] == ['container', 'cp'] for call in docker.calls))
        self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])

    def test_failure_at_each_create_start_copy_exec_phase_cleans_exact_owned_id(self):
        for step in ('create', 'start', 'cp', 'exec'):
            docker = FakeDocker(); docker.fail_on = step
            with self.subTest(step=step), self.fake(docker):
                result = self.run_fake()
            self.assertEqual(result['state'], 'FAILED')
            self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])
            if step == 'create':
                self.assertIn(['container', 'inspect', NAME], docker.calls)

    def test_unclear_identity_retains_container_and_never_removes_foreign_target(self):
        docker = FakeDocker(); docker.fail_on = 'exec'; docker.foreign_cleanup = True
        with self.fake(docker):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['container_cleanup']['state'], 'FAILED')
        self.assertEqual(result['container_cleanup']['retained_container_id'], CID)
        self.assertFalse(any(call[:2] == ['container', 'rm'] for call in docker.calls))

    def test_malformed_forged_qualification_and_cleanup_failure_are_not_success(self):
        for change in ({'qualified': True}, {'binding': {}}, {'state': 'PASSED'}, {'full_cases': []}):
            docker = FakeDocker(); docker.report.update(change)
            with self.subTest(change=change), self.fake(docker):
                result = self.run_fake()
            self.assertEqual(result['state'], 'FAILED')
            self.assertEqual(result['container_cleanup']['state'], 'PASSED')
            self.assertFalse(result['qualified'])
        docker = FakeDocker(); docker.fail_on = 'rm'
        with self.fake(docker):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['failure_code'], 'ISOLATED_CONTAINER_CLEANUP_FAILED')

    def test_inspect_only_image_and_engine_failure_never_create(self):
        docker = FakeDocker()
        docker.fail_on = 'info'
        with self.fake(docker):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertFalse(docker.created)
        self.assertNotIn('container_cleanup', result)
        with self.fake() as (docker, _), patch.object(runner, 'image_contract', side_effect=JourneyError('ISOLATED_IMAGE_PLATFORM')):
            result = self.run_fake()
        self.assertFalse(docker.created)
        self.assertEqual(result['failure_code'], 'ISOLATED_IMAGE_PLATFORM')

    def test_policy_rejection_precedes_docker_transport(self):
        with patch.object(runner, 'local_docker') as docker:
            result = self.run_fake(duration_seconds=1)
        docker.assert_not_called()
        self.assertEqual(result['failure_code'], 'RELIABILITY_DURATION_POLICY')

    def test_second_actual_enforcement_failure_revokes_passed_state(self):
        with self.fake() as (docker, live):
            live.side_effect = [live.return_value, JourneyError('ISOLATED_HOST_SHARED_NAMESPACE')]
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['isolation']['state'], 'FAILED')
        self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])

    def test_final_config_drift_revokes_isolation_state(self):
        with self.fake() as (docker, _):
            original = docker.inspect
            def inspect(kind, identity):
                value = original(kind, identity)
                if kind == 'container' and docker.inspections == 3:
                    value['HostConfig']['Memory'] = 0
                return value
            docker.inspect = inspect
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['isolation']['state'], 'FAILED')

    def test_report_policy_shortcut_or_inner_authority_is_rejected(self):
        for field, value in (('minimum_duration_seconds', 3600), ('requested_duration_seconds', 1),
                             ('requested_concurrent_sessions', 2), ('duration_seconds', 0.01),
                             ('resource_budgets', {}), ('duration_policy_met', False),
                             ('inner_isolation_verified', True), ('requested_disabled_feature_keys', ['harvester'])):
            docker = FakeDocker(); docker.report['reliability'][field] = value
            with self.subTest(field=field), self.fake(docker):
                result = self.run_fake()
            self.assertEqual(result['state'], 'FAILED')
            self.assertEqual(result['container_cleanup']['state'], 'PASSED')

    def test_malformed_create_identity_recovers_only_random_owned_name(self):
        with self.fake() as (docker, _):
            original = docker.command
            def command(args, **options):
                raw = original(args, **options)
                return b'partial-id\n' if args[:2] == ['container', 'create'] else raw
            docker.command = command
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertIn(['container', 'inspect', NAME], docker.calls)
        self.assertNotIn(['container', 'inspect', 'partial-id'], docker.calls)
        self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])

    def test_changed_native_runtime_before_create_prevents_candidate_container(self):
        with self.fake() as (docker, _), patch.object(enforcement, 'verify_runtime',
                side_effect=[{'sha256': 'a' * 64}, JourneyError('ISOLATED_RUNC_IDENTITY_CHANGED')]):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['failure_code'], 'ISOLATED_RUNC_IDENTITY_CHANGED')
        self.assertFalse(docker.created)
        self.assertFalse(any(call[:2] == ['container', 'cp'] for call in docker.calls))

    def test_changed_native_runtime_at_end_revokes_isolation_and_still_cleans(self):
        with self.fake() as (docker, _), patch.object(enforcement, 'verify_runtime',
                side_effect=[{'sha256': 'a' * 64}, {'sha256': 'a' * 64}, JourneyError('ISOLATED_RUNC_IDENTITY_CHANGED')]):
            result = self.run_fake()
        self.assertEqual(result['state'], 'FAILED')
        self.assertEqual(result['isolation']['state'], 'FAILED')
        self.assertEqual(docker.calls[-1], ['container', 'rm', '--force', CID])
