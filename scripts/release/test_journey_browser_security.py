"""Offline security-driver contracts: every server/socket/process is mocked."""
from contextlib import contextmanager
import json
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import journey_browser_ipv6 as ipv6
import journey_browser_security as security
from journey_browser import BrowserTools
from journey_fixture import Fixture, SECRET
from journey_process import JourneyError, installation_identity, private_write


def report(family='ipv4'):
    return {'schema_version': 1, 'state': 'PASSED', 'qualified': False,
            'checks': {name: True for name in security.CHECKS}, 'listener_family': family,
            'listener_count': 2, 'rejected_socket_count': 4, 'rejected_read_count': 8,
            'rejected_mutation_count': 4, 'browser_version': '142.0.1234.5'}


class SecurityContracts(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix='mike-browser-boundary-offline-')
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.fixture = Fixture()
        self.tools = BrowserTools(Path(sys.executable), Path(sys.executable))

    def test_owned_pages_have_no_remote_assets_and_opaque_frame_cannot_inherit_origin(self):
        for path in ('/hostile', '/opaque'):
            status, body = security.fixture_response('GET', path)
            self.assertEqual(status, 200)
            self.assertLess(len(body), 1024)
            self.assertNotIn(b'http:', body)
            self.assertNotIn(b'https:', body)
        opaque = security.PAGES['/opaque']
        self.assertIn(b'sandbox="allow-scripts"', opaque)
        self.assertIn(b'srcdoc=', opaque)
        self.assertNotIn(b'allow-same-origin', opaque)
        for method, path in [('POST', '/hostile'), ('GET', '/'), ('GET', '/hostile?next=http://foreign.test')]:
            self.assertEqual(security.fixture_response(method, path), (404, b''))

    def test_fixture_owns_exact_ipv4_loopback_listener_and_stops_only_its_server(self):
        server = Mock(server_port=12004)
        thread = Mock()
        thread.is_alive.return_value = False
        with patch.object(security, 'ThreadingHTTPServer', return_value=server) as make, \
                patch.object(security.threading, 'Thread', return_value=thread):
            with security.hostile_fixture() as port:
                self.assertEqual(port, 12004)
                self.assertEqual(make.call_args.args[0], ('127.0.0.1', 0))
                thread.start.assert_called_once_with()
            server.shutdown.assert_called_once_with()
            server.server_close.assert_called_once_with()
            thread.join.assert_called_once_with(timeout=3)

    def test_security_report_both_families_stays_partial_and_redacted(self):
        for family in ('ipv4', 'ipv6'):
            value = security.validate_security_report(json.dumps(report(family)).encode(), family)
            self.assertFalse(value['qualified'])
            self.assertEqual(value['listener_family'], family)
            self.assertEqual(value['http_transport'], 'native_browser_redirect_error')
            self.assertFalse(value['raw_browser_logs_retained'])
            self.assertNotIn(self.fixture.session, json.dumps(value))

    def test_security_evidence_rejects_missing_false_extra_wrong_family_or_forged_counts(self):
        edits = [lambda v: v['checks'].pop(security.CHECKS[0]),
                 lambda v: v['checks'].update(native_null_reads_rejected=False),
                 lambda v: v['checks'].update(native_null_reads_rejected=1),
                 lambda v: v['checks'].update(extra=True), lambda v: v.update(qualified=True),
                 lambda v: v.update(listener_family='ipv6'), lambda v: v.update(listener_count=True),
                 lambda v: v.update(rejected_socket_count=3), lambda v: v.update(rejected_read_count=0),
                 lambda v: v.update(rejected_mutation_count=0), lambda v: v.update(browser_version=SECRET),
                 lambda v: v.update(private_log=SECRET)]
        for edit in edits:
            value = report()
            edit(value)
            with self.assertRaises(JourneyError):
                security.validate_security_report(json.dumps(value).encode(), 'ipv4')
        with self.assertRaisesRegex(JourneyError, 'TOO_LARGE'):
            security.validate_security_report(b' ' * 16385, 'ipv4')

    def test_failure_details_never_leak(self):
        for code in (SECRET, {'secret': SECRET}, 'BROWSER_BOUNDARY_OPAQUE'):
            value = {'schema_version': 1, 'state': 'FAILED', 'qualified': False, 'code': code, 'log': SECRET}
            with self.assertRaises(JourneyError) as caught:
                security.validate_security_report(json.dumps(value).encode(), 'ipv4')
            self.assertNotIn(SECRET, str(caught.exception))

    def test_runner_has_private_spec_deadline_clean_environment_and_owned_fixture_cleanup(self):
        @contextmanager
        def fixture():
            yield 11004
        def run(args, cwd, env, timeout):
            value = json.loads(Path(args[2]).read_bytes())
            self.assertEqual(value['target_host'], '::1')
            self.assertEqual(value['hostile_port'], 11004)
            self.assertEqual(Path(args[2]).stat().st_mode & 0o777, 0o600)
            self.assertEqual(env['PATH'], '')
            self.assertNotIn('NODE_OPTIONS', env)
            self.assertEqual(timeout, 100)
            return json.dumps(report('ipv6')).encode()
        with patch.object(security, 'hostile_fixture', side_effect=fixture), \
                patch.object(security, 'captured_run', side_effect=run):
            value = security.run_browser_security(self.tools, [11001, 11002, 11003], self.fixture.session,
                                                  42, self.fixture, self.root, self.root, host='::1')
        self.assertEqual(value['listener_family'], 'ipv6')
        self.assertFalse((self.root / 'browser-boundary-fixture.json').exists())

    def test_runtime_errors_clean_private_spec(self):
        @contextmanager
        def fixture():
            yield 11004
        with patch.object(security, 'hostile_fixture', side_effect=fixture), \
                patch.object(security, 'captured_run', side_effect=JourneyError('CHILD_TIMEOUT')):
            with self.assertRaisesRegex(JourneyError, 'CHILD_TIMEOUT'):
                security.run_browser_security(self.tools, [11001, 11002, 11003], self.fixture.session,
                                              42, self.fixture, self.root, self.root)
        self.assertFalse((self.root / 'browser-boundary-fixture.json').exists())

    def test_invalid_host_never_creates_a_listener(self):
        for host in ('0.0.0.0', '::', 'localhost', 'foreign.test'):
            with patch.object(security, 'hostile_fixture') as fixture, self.assertRaises(JourneyError):
                security.run_browser_security(self.tools, [11001, 11002, 11003], self.fixture.session,
                                              42, self.fixture, self.root, self.root, host=host)
            fixture.assert_not_called()

    @unittest.skipUnless(shutil.which('node'), 'existing Node unavailable; no download attempted')
    def test_offline_node_contracts_and_syntax(self):
        node = shutil.which('node')
        for name in ('journey_browser_security.mjs', 'journey_browser_onboarding.mjs'):
            result = subprocess.run([node, '--check', security.RUNNER.with_name(name)], capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
        result = subprocess.run([node, '--test', security.RUNNER.with_name('journey_browser_security_contracts.mjs')],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, (result.stdout + result.stderr).decode())


class IPv6Contracts(unittest.TestCase):
    def test_only_v6only_loopback_ports_are_reserved_and_closed(self):
        listeners = [Mock() for _ in range(3)]
        for port, listener in enumerate(listeners, 11001):
            listener.getsockname.return_value = ('::1', port, 0, 0)
        with patch.object(ipv6.socket, 'socket', side_effect=listeners) as make:
            with ipv6.reserved_ipv6_ports() as (ports, owned):
                self.assertEqual(ports, [11001, 11002, 11003])
                self.assertEqual(owned, listeners)
            self.assertEqual(make.call_count, 3)
            for call in make.call_args_list:
                self.assertEqual(call.args, (socket.AF_INET6, socket.SOCK_STREAM))
            for listener in listeners:
                listener.setsockopt.assert_called_once_with(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
                listener.bind.assert_called_once_with(('::1', 0))
                listener.close.assert_called_once_with()

    def test_ipv6_unavailable_has_no_ipv4_or_wildcard_fallback_and_closes_partial_reservations(self):
        listener = Mock()
        listener.bind.side_effect = OSError(SECRET)
        with patch.object(ipv6.socket, 'socket', return_value=listener) as make:
            with self.assertRaisesRegex(JourneyError, 'BROWSER_IPV6_LOOPBACK_UNAVAILABLE'):
                with ipv6.reserved_ipv6_ports():
                    self.fail('unavailable listener yielded')
        make.assert_called_once_with(socket.AF_INET6, socket.SOCK_STREAM)
        listener.close.assert_called_once_with()

    def test_ipv6_runtime_io_failure_is_not_mislabeled_as_listener_unavailability(self):
        listeners = [Mock() for _ in range(3)]
        for port, listener in enumerate(listeners, 11001):
            listener.getsockname.return_value = ('::1', port, 0, 0)
        with patch.object(ipv6.socket, 'socket', side_effect=listeners):
            with self.assertRaisesRegex(OSError, 'runtime failure'):
                with ipv6.reserved_ipv6_ports():
                    raise OSError('runtime failure')
        for listener in listeners:
            listener.close.assert_called_once_with()

    def test_ipv6_config_changes_only_product_listeners_and_retains_private_upstream_database(self):
        with tempfile.TemporaryDirectory() as temporary:
            config, home, cwd = ipv6.ipv6_config(Path(temporary) / 'ipv6', [11001, 11002, 11003], 11004,
                                               'postgres://fixture@127.0.0.1:6432/owned')
            value = config.read_text()
            for port in (11001, 11002, 11003):
                self.assertIn(f'[::1]:{port}', value)
            self.assertIn('http://127.0.0.1:11004', value)
            self.assertIn('postgres://fixture@127.0.0.1:6432/owned', value)
            self.assertEqual(config.stat().st_mode & 0o777, 0o600)
            self.assertTrue((home / '.env').exists() and (cwd / '.env').exists())

    def test_ipv6_health_must_match_private_candidate_pid_generation_and_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / 'config.toml'
            identity, location = installation_identity(config)
            location.parent.mkdir(parents=True)
            record = {'instance_id': identity, 'pid': 777, 'generation': 'owned-generation'}
            private_write(location, json.dumps(record).encode())
            health = {'instance': {'instance_id': identity, 'boot_generation': 'owned-generation'},
                      'build': {'sha': 'a' * 40}, 'persistence': {'enabled': True, 'reachable': True}, 'status': 'ok'}
            child = Mock(pid=777)
            child.poll.return_value = None
            with patch.object(ipv6, 'ipv6_json', return_value=health) as request:
                result = ipv6.wait_ipv6_ready(child, config, 11002, 'a' * 40)
                self.assertTrue(result['owned_pid_and_generation'])
                request.assert_called_once_with(11002, '/api/health')
                child.pid = 778
                with self.assertRaisesRegex(JourneyError, 'PRIVATE_IDENTITY_MISMATCH'):
                    ipv6.wait_ipv6_ready(child, config, 11002, 'a' * 40)

    def test_ipv6_readiness_cannot_adopt_a_dead_or_unowned_process(self):
        child = Mock()
        child.poll.return_value = 1
        with patch.object(ipv6, 'ipv6_json') as request:
            with self.assertRaisesRegex(JourneyError, 'CANDIDATE_EXITED'):
                ipv6.wait_ipv6_ready(child, '/tmp/owned-config.toml', 11002, 'a' * 40)
        request.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)
