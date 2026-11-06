"""Owned hostile page fixture and allowlisted native-browser boundary evidence.

This module never adopts a web server, fetches tooling, or claims a full AC pass.
The caller owns and identifies the actual candidate listeners and capture DB.
"""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import re
import threading

from common import sha256
from journey_browser import browser_spec
from journey_process import JourneyError, captured_run, private_write, require, runtime_environment

RUNNER = Path(__file__).with_suffix('.mjs')
CHECKS = ('native_hostile_reads_rejected', 'native_null_reads_rejected',
          'native_hostile_mutations_rejected', 'native_null_mutations_rejected',
          'session_pin_unchanged', 'capture_bytes_unchanged', 'rejected_sockets_receive_no_events',
          'same_origin_reads_and_mutations', 'same_origin_live_capture_events', 'native_origin_headers')
FAILURES = frozenset({'BROWSER_BOUNDARY_SPEC_INVALID', 'BROWSER_BOUNDARY_TIMEOUT',
                     'BROWSER_TOOLING_UNAVAILABLE', 'BROWSER_LAUNCH_UNAVAILABLE',
                     'BROWSER_BOUNDARY_SAME_ORIGIN', 'BROWSER_BOUNDARY_HOSTILE',
                     'BROWSER_BOUNDARY_OPAQUE', 'BROWSER_BOUNDARY_SIDE_EFFECTS', 'BROWSER_BOUNDARY_LIVE_EVENT'})
PAGES = {
    '/hostile': b'<!doctype html><title>Owned hostile-origin fixture</title>'
                b'<link rel="icon" href="data:,"><p>Synthetic boundary fixture</p>',
    '/opaque': b'<!doctype html><title>Owned opaque-origin fixture</title>'
               b'<link rel="icon" href="data:,"><iframe sandbox="allow-scripts" srcdoc="<!doctype html><title>Opaque fixture</title>"></iframe>',
}


def fixture_response(method, path):
    if method != 'GET' or path not in PAGES:
        return 404, b''
    return 200, PAGES[path]


@contextmanager
def hostile_fixture():
    class Handler(BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.1'
        def setup(self):
            super().setup()
            self.connection.settimeout(5)
        def log_message(self, *_args):
            pass
        def do_GET(self):
            status, body = fixture_response('GET', self.path)
            self.send_response(status)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Cache-Control', 'no-store')
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(body)
            self.close_connection = True
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_port
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)
        require(not thread.is_alive(), 'BROWSER_HOSTILE_FIXTURE_CLEANUP')


def validate_security_report(raw, family):
    require(len(raw) <= 16384, 'BROWSER_BOUNDARY_REPORT_TOO_LARGE')
    value = json.loads(raw)
    require(isinstance(value, dict) and type(value.get('schema_version')) is int and
            value['schema_version'] == 1 and value.get('qualified') is False, 'BROWSER_BOUNDARY_REPORT_SCHEMA')
    if value.get('state') == 'FAILED':
        code = value.get('code')
        raise JourneyError(code if isinstance(code, str) and code in FAILURES else 'BROWSER_BOUNDARY_CHECK_FAILED')
    fields = {'schema_version', 'state', 'qualified', 'checks', 'listener_family', 'listener_count',
              'rejected_socket_count', 'rejected_read_count', 'rejected_mutation_count', 'browser_version'}
    require(set(value) == fields and value['state'] == 'PASSED', 'BROWSER_BOUNDARY_REPORT_SCHEMA')
    require(isinstance(value['checks'], dict) and set(value['checks']) == set(CHECKS) and
            all(item is True for item in value['checks'].values()), 'BROWSER_BOUNDARY_CHECK_SET')
    require(family in ('ipv4', 'ipv6') and value['listener_family'] == family, 'BROWSER_BOUNDARY_FAMILY')
    counts = {'listener_count': 2, 'rejected_socket_count': 4, 'rejected_read_count': 8, 'rejected_mutation_count': 4}
    require(all(type(value[key]) is int and value[key] == count for key, count in counts.items()),
            'BROWSER_BOUNDARY_OBSERVATION_COUNT')
    require(isinstance(value['browser_version'], str) and
            re.fullmatch(r'\d{1,4}(?:\.\d{1,6}){1,4}', value['browser_version']), 'BROWSER_BOUNDARY_VERSION')
    return {'kind': 'INSTALLED_CHROMIUM_NATIVE_ORIGIN_BOUNDARY', 'qualified': False,
            'listener_family': family, 'checks': {key: True for key in CHECKS}, **counts,
            'browser_version': value['browser_version'], 'http_transport': 'native_browser_redirect_error',
            'navigation_transport': 'owned_response_passthrough_redirects_refused',
            'opaque_origin': 'sandbox_allow_scripts_without_allow_same_origin',
            'server_rejection_status': 403, 'raw_browser_logs_retained': False,
            'fresh_browser_context': True, 'chromium_sandbox_requested': True}


def run_browser_security(tools, ports, session, request_id, fixture, home, cwd, *, host='127.0.0.1'):
    require(host in ('127.0.0.1', '::1'), 'BROWSER_BOUNDARY_HOST')
    spec = browser_spec(tools, ports, session, request_id, fixture)
    with hostile_fixture() as hostile_port:
        require(hostile_port not in ports, 'BROWSER_HOSTILE_PORT_COLLISION')
        spec.update(target_host=host, hostile_port=hostile_port)
        location = Path(home) / 'browser-boundary-fixture.json'
        private_write(location, (json.dumps(spec) + '\n').encode())
        try:
            raw = captured_run([str(tools.node), str(RUNNER), str(location)], cwd,
                               runtime_environment(home), timeout=100)
            result = validate_security_report(raw, 'ipv6' if host == '::1' else 'ipv4')
            result.update(runner_sha256=sha256(RUNNER), exit_status=0, separately_owned_hostile_origin=True,
                          helper_sha256={name: sha256(RUNNER.with_name(name)) for name in
                                         ('journey_browser.mjs', 'journey_browser_onboarding.mjs')})
            return result
        finally:
            location.unlink(missing_ok=True)
