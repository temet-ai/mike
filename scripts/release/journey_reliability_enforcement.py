"""Host-observed runtime enforcement, before any candidate payload is released."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re

from journey_process import JourneyError, require
from journey_reliability_payload import NAMESPACES, local_namespaces
from journey_reliability_native_runtime import runtime_evidence, verify_runtime
from journey_reliability_runner_contract import GID, MEMORY_BYTES, TMPFS_BYTES, UID


def daemon_contract(docker, runtime_name):
    try:
        value = json.loads(docker.command(['info', '--format', '{{json .}}']))
    except (ValueError, UnicodeError) as error:
        raise JourneyError('ISOLATED_DAEMON_INFO_JSON') from error
    require(isinstance(value, dict) and value.get('OSType') == 'linux'
            and value.get('Architecture') == 'x86_64' and value.get('CgroupVersion') == '2',
            'ISOLATED_DAEMON_NATIVE_CGROUP_V2_REQUIRED')
    security = value.get('SecurityOptions')
    require(isinstance(security, list) and all(isinstance(item, str) for item in security)
            and any(item.startswith('name=seccomp,') for item in security)
            and not any(item.startswith('name=rootless') for item in security),
            'ISOLATED_DAEMON_SECURITY_REQUIRED')
    for key in ('MemoryLimit', 'SwapLimit', 'PidsLimit', 'CpuCfsPeriod', 'CpuCfsQuota'):
        require(value.get(key) is True, 'ISOLATED_DAEMON_RESOURCE_ENFORCEMENT_REQUIRED')
    peer = getattr(docker, 'verified_peer', None)
    require(isinstance(peer, dict) and isinstance(peer.get('executable'), dict)
            and re.fullmatch('[0-9a-f]{64}', peer['executable'].get('sha256', '')),
            'ISOLATED_VERIFIED_DAEMON_PEER_REQUIRED')
    identity = verify_runtime(value, runtime_name, getattr(docker, 'verified_native_runtime', None))
    docker.verified_native_runtime = identity
    return {**runtime_evidence(identity), 'direct_native_dockerd_peer_verified': True,
            'daemon_host_filesystem_view_verified': True, 'daemon_executable_sha256': peer['executable']['sha256'],
            'daemon_capabilities_observed': True, 'cgroup_version': 2,
            'rootless_supported': False, 'seccomp_available': True}


def read_bounded(path, limit=131072):
    with path.open('rb') as source:
        value = source.read(limit + 1)
    require(len(value) <= limit, 'ISOLATED_HOST_OBSERVATION_LIMIT')
    return value


def process_identity(root):
    value = read_bounded(root / 'stat', 4096).decode('ascii')
    boundary = value.rfind(') ')
    require(boundary > 0, 'ISOLATED_HOST_PROCESS_STAT')
    fields = value[boundary + 2:].split()
    require(len(fields) >= 20 and fields[19].isdigit(), 'ISOLATED_HOST_PROCESS_STAT')
    return fields[19]


def status_contract(value):
    require(re.search(r'^Uid:\s+' + (str(UID) + r'\s+') * 3 + str(UID) + r'$', value, re.MULTILINE)
            and re.search(r'^Gid:\s+' + (str(GID) + r'\s+') * 3 + str(GID) + r'$', value, re.MULTILINE),
            'ISOLATED_HOST_PROCESS_NONROOT')
    groups = re.findall(r'^Groups:[ \t]*(.*)$', value, re.MULTILINE)
    require(len(groups) == 1 and set(groups[0].split()) <= {str(GID)}, 'ISOLATED_HOST_SUPPLEMENTARY_GROUPS')
    for key in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb'):
        require(re.search(r'^' + key + r':\s+0+$', value, re.MULTILINE), 'ISOLATED_HOST_PROCESS_CAPABILITIES')
    require(re.search(r'^NoNewPrivs:\s+1$', value, re.MULTILINE)
            and re.search(r'^Seccomp:\s+2$', value, re.MULTILINE), 'ISOLATED_HOST_PROCESS_SECURITY')


def mount_contract(value):
    rows = [line.split() for line in value.splitlines()]
    roots, work = ([row for row in rows if len(row) > 6 and row[4] == mount] for mount in ('/', '/work'))
    require(len(roots) == 1 and 'ro' in roots[0][5].split(','), 'ISOLATED_HOST_ROOT_FILESYSTEM')
    require(len(work) == 1 and '-' in work[0], 'ISOLATED_HOST_TMPFS')
    options = set(work[0][5].split(','))
    separator = work[0].index('-')
    require(set(('rw', 'nosuid', 'nodev')) <= options and 'noexec' not in options
            and work[0][separator + 1] == 'tmpfs', 'ISOLATED_HOST_TMPFS')
    super_options = work[0][separator + 3].split(',')
    sizes = [item[5:] for item in super_options if item.startswith('size=')]
    require(len(sizes) == 1 and sizes[0] in (str(TMPFS_BYTES), str(TMPFS_BYTES // 1024) + 'k'),
            'ISOLATED_HOST_TMPFS_SIZE')


def cgroup_contract(process_root, pid, cgroup_root):
    rows = read_bounded(process_root / 'cgroup', 16384).decode('ascii').splitlines()
    require(len(rows) == 1 and rows[0].startswith('0::/'), 'ISOLATED_HOST_CGROUP_V2')
    relative = rows[0][4:]
    require(relative and '..' not in PurePosixPath(relative).parts and not relative.startswith('/'),
            'ISOLATED_HOST_CGROUP_PATH')
    group = cgroup_root / relative
    require(not any(path.is_symlink() for path in (group, *group.parents)), 'ISOLATED_HOST_CGROUP_SYMLINK')
    read = lambda name: read_bounded(group / name, 16384).decode('ascii').strip()
    require(str(pid) in read('cgroup.procs').splitlines(), 'ISOLATED_HOST_CGROUP_PROCESS')
    require(read('memory.max') == str(MEMORY_BYTES) and read('memory.swap.max') == '0'
            and read('pids.max') == '256', 'ISOLATED_HOST_CGROUP_LIMITS')
    cpu = read('cpu.max').split()
    require(len(cpu) == 2 and all(item.isdigit() for item in cpu)
            and int(cpu[1]) > 0 and int(cpu[0]) == int(cpu[1]) * 2, 'ISOLATED_HOST_CPU_LIMIT')
    return hashlib.sha256(relative.encode()).hexdigest()


def live_container_contract(value, command, *, proc_root=Path('/proc'), cgroup_root=Path('/sys/fs/cgroup')):
    """Read the actual local host process, independent of in-container claims."""
    try:
        pid = value.get('State', {}).get('Pid')
        require(type(pid) is int and pid > 1, 'ISOLATED_HOST_PROCESS_REQUIRED')
        root = proc_root / str(pid)
        started = process_identity(root)
        expected = ['/usr/bin/python3', *command]
        actual = read_bounded(root / 'cmdline', 65536).split(b'\0')
        require(actual == [item.encode() for item in expected] + [b''], 'ISOLATED_HOST_PROCESS_COMMAND')
        host = local_namespaces()
        observed = {key: os.readlink(root / 'ns' / key) for key in NAMESPACES}
        require(all(re.fullmatch(key + r':\[[0-9]+\]', observed[key]) and observed[key] != host[key]
                    for key in NAMESPACES), 'ISOLATED_HOST_SHARED_NAMESPACE')
        status_contract(read_bounded(root / 'status').decode('ascii'))
        mount_contract(read_bounded(root / 'mountinfo').decode('ascii'))
        interfaces = {line.split(':', 1)[0].strip() for line in read_bounded(root / 'net/dev').decode('ascii').splitlines()
                      if ':' in line}
        require(interfaces == {'lo'}, 'ISOLATED_HOST_NETWORK_INTERFACE')
        for line in read_bounded(root / 'net/route').decode('ascii').splitlines()[1:]:
            require(not line.strip() or line.split()[0] == 'lo', 'ISOLATED_HOST_NETWORK_ROUTE')
        cgroup = cgroup_contract(root, pid, cgroup_root)
        require(process_identity(root) == started
                and all(os.readlink(root / 'ns' / key) == observed[key] for key in NAMESPACES),
                'ISOLATED_HOST_PROCESS_CHANGED')
        return {'host_process_boundary_observed': True, 'host_process_id': pid,
                'process_start_ticks': int(started), 'cgroup_enforcement_observed': True,
                'cgroup_path_sha256': cgroup, 'all_transport_isolation_verified': True,
                'namespace_identity_sha256': hashlib.sha256(json.dumps(observed, sort_keys=True).encode()).hexdigest()}
    except (OSError, ValueError, UnicodeError, IndexError) as error:
        raise JourneyError('ISOLATED_HOST_ENFORCEMENT_UNAVAILABLE') from error
