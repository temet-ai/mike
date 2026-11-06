"""Private image entrypoint. No candidate, database or listener starts on import."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import re
import pwd
import signal
import stat
import sys
import time

from archive import verify_candidate
from inputs import extract_pair
from journey import check, new_result, verify_binding
from journey_persistent import owned_cluster, verified_migration_inventory
from journey_process import JourneyError, captured_run, owned_child, require
from journey_reliability_payload import MAX_RESULT_BYTES, NAMESPACES
from journey_reliability_profiles import NORMAL_PROFILE, reliability_options
from journey_reliability_runner_contract import GID, UID

WORK = Path('/work')
PG_TOOLS = Path('/usr/lib/postgresql/19/bin')
PG_PORT = 55439


class ContainerAdmission:
    """Live inner cross-check, never a replacement for outer daemon inspection."""
    def __init__(self, host_namespaces):
        require(type(host_namespaces) is dict and set(host_namespaces) == set(NAMESPACES),
                "ISOLATED_HOST_NAMESPACE_CONTRACT")
        self.host_namespaces = dict(host_namespaces)
        self.observation = observe_boundary(self.host_namespaces)


def require_admission(admission):
    require(type(admission) is ContainerAdmission, "NORMAL_DEFAULT_VERIFIED_CONTAINER_REQUIRED")
    require(observe_boundary(admission.host_namespaces) == admission.observation,
            "NORMAL_DEFAULT_CONTAINER_BOUNDARY_CHANGED")
    return dict(admission.observation)


def observe_boundary(host_namespaces):
    require(os.getuid() == UID and os.getgid() == GID and os.geteuid() == UID,
            "ISOLATED_CONTAINER_NONROOT_REQUIRED")
    namespaces = {}
    for key in NAMESPACES:
        host = host_namespaces.get(key)
        current = os.readlink('/proc/self/ns/' + key)
        require(isinstance(host, str) and re.fullmatch(key + r":\[[0-9]+\]", host)
                and re.fullmatch(key + r":\[[0-9]+\]", current) and current != host,
                "ISOLATED_CONTAINER_SHARED_NAMESPACE")
        namespaces[key] = current
    status = Path('/proc/self/status').read_text()
    groups = re.findall(r'^Groups:[ \t]*(.*)$', status, re.MULTILINE)
    require(len(groups) == 1 and set(groups[0].split()) <= {str(GID)}, 'ISOLATED_CONTAINER_SUPPLEMENTARY_GROUPS')
    for key in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb'):
        require(re.search(r'^' + key + r':\s+0+$', status, re.MULTILINE), "ISOLATED_CONTAINER_CAPABILITIES")
    require(re.search(r'^NoNewPrivs:\s+1$', status, re.MULTILINE)
            and re.search(r'^Seccomp:\s+2$', status, re.MULTILINE), "ISOLATED_CONTAINER_PROCESS_SECURITY")
    interfaces = {line.split(':', 1)[0].strip() for line in Path('/proc/net/dev').read_text().splitlines() if ':' in line}
    require(interfaces == {'lo'}, "ISOLATED_CONTAINER_NETWORK_INTERFACE")
    routes = Path('/proc/net/route').read_text().splitlines()[1:]
    require(all(line.split()[0] == 'lo' for line in routes if line.strip()), "ISOLATED_CONTAINER_NETWORK_ROUTE")
    mounts = [line.split() for line in Path('/proc/self/mountinfo').read_text().splitlines()]
    roots = [row for row in mounts if len(row) > 6 and row[4] == '/']
    work = [row for row in mounts if len(row) > 6 and row[4] == '/work']
    require(len(roots) == 1 and 'ro' in roots[0][5].split(','), "ISOLATED_CONTAINER_ROOT_WRITABLE")
    require(len(work) == 1 and set(('rw', 'nosuid', 'nodev')) <= set(work[0][5].split(','))
            and work[0][work[0].index('-') + 1] == 'tmpfs', "ISOLATED_CONTAINER_WORK_TMPFS")
    require(not Path('/var/run/docker.sock').exists() and not Path('/run/docker.sock').exists(),
            "ISOLATED_CONTAINER_DOCKER_SOCKET")
    return {"inner_boundary_observed": True, "outer_daemon_inspection_required": True,
            "distinct_host_namespaces": True, "only_loopback_interface": True,
            "nonroot_uid": UID, "root_filesystem_readonly": True,
            "capabilities_empty": True, "no_new_privileges": True, "seccomp_filter_present": True}


def image_preflight():
    require((3, 11) <= sys.version_info[:2] < (3, 14), "ISOLATED_IMAGE_PYTHON_VERSION")
    try:
        account = pwd.getpwuid(UID)
    except KeyError as error:
        raise JourneyError("ISOLATED_IMAGE_PASSWD_ENTRY_REQUIRED") from error
    require(account.pw_uid == UID and account.pw_gid == GID, "ISOLATED_IMAGE_PASSWD_ENTRY_REQUIRED")
    environment = {'PATH': str(PG_TOOLS) + ':/usr/bin:/bin', 'HOME': '/work/home',
                   'TMPDIR': '/work/tmp', 'LC_ALL': 'C', 'LANG': 'C'}
    for directory in ('home', 'tmp'):
        (WORK / directory).mkdir(mode=0o700)
    versions = {}
    for name in ('initdb', 'postgres', 'psql', 'pg_dump', 'pg_restore'):
        path = PG_TOOLS / name
        require(path.is_file() and os.access(path, os.X_OK), "ISOLATED_IMAGE_PG19_TOOLS_MISSING")
        output = captured_run([str(path), '--version'], WORK, environment, timeout=10).decode('ascii')
        match = re.fullmatch(r'[^\n]*\(PostgreSQL\) (19(?:\.[0-9]+|(?:beta|rc)[0-9]+))(?: [^\n]*)?\n?', output)
        require(match is not None, "ISOLATED_IMAGE_PG19_VERSION")
        versions[name] = match.group(1)
    require(len(set(versions.values())) == 1, "ISOLATED_IMAGE_PG19_TOOL_MISMATCH")
    require(Path('/usr/bin/uname').is_file() or Path('/bin/uname').is_file(), "ISOLATED_IMAGE_UNAME_REQUIRED")
    return environment, {'python_version': '.'.join(map(str, sys.version_info[:3])),
                         'postgres_version': versions['postgres'], 'compatible_tools_observed': True}


@contextmanager
def private_postgres(environment, admission):
    require_admission(admission)
    data = WORK / 'pgdata'
    captured_run([str(PG_TOOLS / 'initdb'), '-D', str(data), '-U', 'postgres',
                  '--auth-local=reject', '--auth-host=trust', '--no-locale', '--encoding=UTF8'],
                 WORK, environment, timeout=60)
    os.environ.update(MIKE_TEST_ADMIN_URL=f'postgresql://postgres@127.0.0.1:{PG_PORT}/postgres?sslmode=disable',
                      MIKE_RECOVERY_TEST_PGDATA=str(data), MIKE_RECOVERY_TEST_TOOL_DIR=str(PG_TOOLS))
    with owned_child([str(PG_TOOLS / 'postgres'), '-D', str(data), '-h', '127.0.0.1', '-p', str(PG_PORT),
                      '-c', 'unix_socket_directories=', '-c', 'max_connections=80', '-c', 'shared_buffers=128MB'],
                     WORK, environment) as child:
        deadline, cluster = time.monotonic() + 60, None
        while cluster is None:
            require(child.poll() is None, 'ISOLATED_POSTGRES_EXITED')
            try:
                cluster = owned_cluster()
            except JourneyError:
                require(time.monotonic() < deadline, 'ISOLATED_POSTGRES_READINESS_TIMEOUT')
                time.sleep(0.25)
        try:
            extensions = json.loads(cluster.sql('postgres', "BEGIN READ ONLY; SELECT "
                "coalesce(json_object_agg(name,default_version),'{}'::json)::text "
                "FROM pg_available_extensions WHERE name IN ('vector','pgcrypto','pg_trgm'); COMMIT;"))
            require(isinstance(extensions, dict) and set(extensions) == {'vector', 'pgcrypto', 'pg_trgm'}
                    and all(isinstance(version, str) and re.fullmatch(r'[0-9]+(?:\.[0-9]+)+', version)
                            for version in extensions.values()), 'ISOLATED_IMAGE_PG_EXTENSIONS_REQUIRED')
            yield cluster
        finally:
            cluster.cleanup()
            child.signal_group(signal.SIGTERM)
            require(child.wait(timeout=30) == 0, 'ISOLATED_POSTGRES_SHUTDOWN_FAILED')


def run_payload(launch):
    from journey_reliability import new_reliability_report, reliability_policy
    from journey_reliability_normal import run_bound

    result = new_result(None)
    result.update(runner='isolated-normal-default-reliability-helper', limits=[])
    try:
        require(launch.get('schema_version') == 1, 'ISOLATED_LAUNCH_VERSION')
        request = launch['request']
        policy = reliability_policy(request['duration_seconds'], request['concurrent_sessions'])
        options = reliability_options(NORMAL_PROFILE, request['resource_budgets'])
        result['reliability'] = new_reliability_report(policy, **options)
        admission = ContainerAdmission(launch['host_namespaces'])
        check(result, 'archive_normal_container_inner_boundary', lambda: require_admission(admission))
        environment, capabilities = image_preflight()
        check(result, 'archive_normal_image_capabilities', lambda: capabilities)
        candidate = WORK / 'payload' / 'candidate'
        # This is the first candidate execution; all previous steps only ran the
        # explicitly trusted image Python/PostgreSQL capability commands.
        manifest = check(result, 'archive_normal_candidate_verification', lambda: verify_candidate(candidate))
        installed = WORK / 'installed'
        installed.mkdir(mode=0o700)
        require(extract_pair(candidate, manifest, installed) == launch['manifest_sha256'],
                'ISOLATED_INSTALLED_MANIFEST_CHANGED')
        paths, result['binding'] = verify_binding(installed, manifest, launch['manifest_sha256'])
        inventory = verified_migration_inventory(paths, manifest)
        with private_postgres(environment, admission) as cluster:
            check(result, 'archive_normal_default_bounded_run', lambda: run_bound(
                paths, manifest, WORK / 'reliability', result, cluster, inventory, policy, isolation=admission))
        check(result, 'archive_normal_postgres_cleanup', lambda: {'owned_cluster_stopped': True})
    except Exception as error:
        result.update(state='FAILED', failure_type=type(error).__name__)
        if isinstance(error, JourneyError) and re.fullmatch(r'[A-Z][A-Z0-9_]{0,127}', str(error)):
            result['failure_code'] = str(error)
        if 'reliability' in result:
            result['reliability']['failed_run_count'] = 1
    return result


def main(launch):
    # Catches entry failures into a bounded result while the bootstrap remains
    # alive. A stopped container would lose its tmpfs and its evidence.
    try:
        result = run_payload(launch)
        encoded = json.dumps(result, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()
        require(len(encoded) <= MAX_RESULT_BYTES, 'ISOLATED_RESULT_LIMIT')
    except Exception:
        encoded = b'{"state":"FAILED","qualified":false,"failure_code":"ISOLATED_ENTRYPOINT_FAILED","checks":[]}'
    temporary = WORK / 'result.tmp'
    with temporary.open('xb') as output:
        output.write(encoded)
    temporary.chmod(0o600)
    temporary.rename(WORK / 'result.json')
