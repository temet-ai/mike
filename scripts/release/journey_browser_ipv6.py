"""Optional V6ONLY loopback candidate, reusing only this browser journey's owned DB.

The IPv4 daemon must be stopped before entry. No adopted listeners, wildcard
binds, daemon restart, OS networking changes, or fallback to another address.
"""
from contextlib import contextmanager
import http.client
import json
from pathlib import Path
import socket
import stat
import time

from common import sha256
from journey import check, create_config
from journey_browser_security import run_browser_security
from journey_persistent import graceful_stop, migration_ledger
from journey_process import (JourneyError, installation_identity, owned_child, require,
                             runtime_environment, validate_identity)


@contextmanager
def reserved_ipv6_ports():
    listeners = []
    try:
        try:
            for _ in range(3):
                listener = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
                listeners.append(listener)
                listener.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
                listener.bind(('::1', 0))
        except OSError as error:
            raise JourneyError('BROWSER_IPV6_LOOPBACK_UNAVAILABLE') from error
        yield [listener.getsockname()[1] for listener in listeners], listeners
    finally:
        for listener in listeners:
            listener.close()


def ipv6_json(port, path):
    require(type(port) is int and 0 < port < 65536 and path == '/api/health', 'BROWSER_IPV6_HEALTH_TARGET')
    connection = http.client.HTTPConnection('::1', port, timeout=3)
    try:
        connection.request('GET', path)
        response = connection.getresponse()
        raw = response.read(1024 * 1024 + 1)
        require(response.status == 200 and len(raw) <= 1024 * 1024, 'BROWSER_IPV6_HEALTH_RESPONSE')
        return json.loads(raw)
    finally:
        connection.close()


def wait_ipv6_ready(child, config, port, source_commit, timeout=30):
    identity, record_path = installation_identity(config)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        require(child.poll() is None, 'BROWSER_IPV6_CANDIDATE_EXITED')
        try:
            metadata = record_path.lstat()
            require(stat.S_ISREG(metadata.st_mode) and metadata.st_mode & 0o077 == 0,
                    'BROWSER_IPV6_IDENTITY_PERMISSIONS')
            record = json.loads(record_path.read_bytes())
            health = ipv6_json(port, '/api/health')
            validate_identity(record, health, identity, child.pid, source_commit, persistent=True)
            return {'owned_pid_and_generation': True, 'listener_family': 'ipv6', 'loopback_only': True}
        except (FileNotFoundError, ConnectionError, TimeoutError, OSError, http.client.HTTPException):
            time.sleep(0.05)
    raise JourneyError('BROWSER_IPV6_READINESS_TIMEOUT')


def ipv6_config(root, ports, upstream_port, database_url):
    config, home, cwd = create_config(root, ports, upstream_port, database_url=database_url)
    value = config.read_text()
    for port in ports:
        value = value.replace(json.dumps(f'127.0.0.1:{port}'), json.dumps(f'[::1]:{port}'))
    config.write_text(value)
    return config, home, cwd


def run_ipv6_boundary(paths, manifest, root, result, tools, cluster, database, inventory,
                      fixture, upstream_port, request_id):
    with reserved_ipv6_ports() as (ports, reservations):
        config, home, cwd = ipv6_config(root, ports, upstream_port, cluster.config_url(database))
        before = {item: sha256(item) for item in (config, home / '.env', cwd / '.env')}
        ledger = migration_ledger(cluster, database, inventory)
        for reservation in reservations:
            reservation.close()
        with owned_child([str(paths['mike']), '--config', str(config), 'serve'],
                         cwd, runtime_environment(home)) as child:
            ready = lambda: wait_ipv6_ready(child, config, ports[1], manifest['source_commit'])
            check(result, 'archive_browser_ipv6_owned_readiness', ready)
            def browser_check():
                ready()
                observation = run_browser_security(tools, ports, fixture.session, request_id,
                                                   fixture, home, cwd, host='::1')
                ready()
                return observation
            check(result, 'archive_browser_ipv6_native_boundary', browser_check)
            def continuity():
                require(before == {item: sha256(item) for item in before}, 'BROWSER_IPV6_PRIVATE_FILES_CHANGED')
                require(migration_ledger(cluster, database, inventory) == ledger, 'BROWSER_IPV6_LEDGER_CHANGED')
                return {'private_files_and_ledger_unchanged': True, 'only_browser_owned_database': True}
            check(result, 'archive_browser_ipv6_fixture_continuity', continuity)
            check(result, 'archive_browser_ipv6_owned_stop', lambda: graceful_stop(child))
