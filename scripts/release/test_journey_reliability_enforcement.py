"""Offline host-observed namespace/cgroup contracts using plain fixture files."""
import json
from pathlib import Path
import tempfile
from unittest.mock import Mock, patch

import journey_reliability_enforcement as enforcement
from journey_process import JourneyError
from journey_reliability_payload import NAMESPACES
from journey_reliability_runner_contract import GID, MEMORY_BYTES, TMPFS_BYTES, UID
from test_journey_reliability import OfflineTests


def daemon():
    return {'OSType': 'linux', 'Architecture': 'x86_64', 'CgroupVersion': '2',
            'Runtimes': {'mike-release-native': {'path': '/usr/bin/runc'}},
            'SecurityOptions': ['name=seccomp,profile=builtin'], 'MemoryLimit': True,
            'SwapLimit': True, 'PidsLimit': True, 'CpuCfsPeriod': True, 'CpuCfsQuota': True}


def process(root):
    proc = root / 'proc'; proc.mkdir()
    selected = proc / '42'; selected.mkdir()
    status = 'Uid:\t' + '\t'.join([str(UID)] * 4) + '\nGid:\t' + '\t'.join([str(GID)] * 4) + '\n'
    status += ''.join(key + ':\t0000000000000000\n' for key in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb'))
    (selected / 'status').write_text(status + 'Groups:\t' + str(GID) + '\nNoNewPrivs:\t1\nSeccomp:\t2\n')
    (selected / 'stat').write_text('42 (python3) ' + ' '.join(['S'] + ['0'] * 18 + ['123'] + ['0'] * 30))
    (selected / 'cmdline').write_bytes(b'/usr/bin/python3\x00-I\x00')
    (selected / 'mountinfo').write_text('1 0 0:1 / / ro - overlay overlay ro\n'
        + f'2 1 0:2 / /work rw,nosuid,nodev - tmpfs tmpfs rw,size={TMPFS_BYTES // 1024}k\n')
    (selected / 'cgroup').write_text('0::/owned\n')
    (selected / 'net').mkdir()
    (selected / 'net/dev').write_text('header\n lo: 0 0 0 0\n')
    (selected / 'net/route').write_text('Iface Destination Gateway\n')
    group = root / 'cgroup'; group.mkdir(); owned = group / 'owned'; owned.mkdir()
    for name, value in {'cgroup.procs': '42\n', 'memory.max': str(MEMORY_BYTES),
                         'memory.swap.max': '0', 'pids.max': '256', 'cpu.max': '200000 100000'}.items():
        (owned / name).write_text(value)
    return proc, group, selected, owned


class EnforcementTests(OfflineTests):
    def setUp(self):
        super().setUp()
        self.enterContext(patch.object(enforcement, "verify_runtime", return_value={"sha256": "a" * 64}))

    def test_engine_must_support_all_enforcement_before_create(self):
        docker = Mock(); docker.verified_peer = {'executable': {'sha256': 'f' * 64}}
        docker.command.return_value = json.dumps(daemon()).encode()
        self.assertTrue(enforcement.daemon_contract(docker, 'mike-release-native')['daemon_capabilities_observed'])
        for key, value in [('CgroupVersion', '1'), ('MemoryLimit', False), ('SwapLimit', False),
                           ('PidsLimit', False), ('CpuCfsQuota', False), ('Architecture', 'aarch64'),
                           ('SecurityOptions', ['name=rootless', 'name=seccomp,profile=builtin'])]:
            changed = daemon(); changed[key] = value; docker.command.return_value = json.dumps(changed).encode()
            with self.subTest(key=key), self.assertRaises(JourneyError):
                enforcement.daemon_contract(docker, 'mike-release-native')

    def test_actual_host_process_namespaces_and_limits_are_checked(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc, group, selected, owned = process(Path(temporary))
            with patch.object(enforcement, 'local_namespaces', return_value={key: key + ':[1]' for key in NAMESPACES}), \
                    patch.object(enforcement.os, 'readlink', side_effect=lambda path: path.name + ':[2]'):
                value = enforcement.live_container_contract({'State': {'Pid': 42}}, ['-I'], proc_root=proc, cgroup_root=group)
                self.assertTrue(value['host_process_boundary_observed'])
                self.assertTrue(value['cgroup_enforcement_observed'])
                self.assertEqual(value['process_start_ticks'], 123)
                for name, invalid in [('memory.max', 'max'), ('memory.swap.max', '100'), ('pids.max', 'max'),
                                       ('cpu.max', 'max 100000'), ('cgroup.procs', '99')]:
                    path = owned / name; previous = path.read_text(); path.write_text(invalid)
                    with self.subTest(name=name), self.assertRaises(JourneyError):
                        enforcement.live_container_contract({'State': {'Pid': 42}}, ['-I'], proc_root=proc, cgroup_root=group)
                    path.write_text(previous)
                (selected / 'net/dev').write_text(' lo: 0\n eth0: 0\n')
                with self.assertRaisesRegex(JourneyError, 'NETWORK_INTERFACE'):
                    enforcement.live_container_contract({'State': {'Pid': 42}}, ['-I'], proc_root=proc, cgroup_root=group)

    def test_missing_permission_shared_namespace_or_noexec_is_not_admission(self):
        with tempfile.TemporaryDirectory() as temporary:
            proc, group, selected, owned = process(Path(temporary))
            with patch.object(enforcement, 'local_namespaces', return_value={key: key + ':[1]' for key in NAMESPACES}), \
                    patch.object(enforcement.os, 'readlink', side_effect=lambda path: path.name + ':[1]'), \
                    self.assertRaisesRegex(JourneyError, 'SHARED_NAMESPACE'):
                enforcement.live_container_contract({'State': {'Pid': 42}}, ['-I'], proc_root=proc, cgroup_root=group)
            with patch.object(enforcement.os, 'readlink', side_effect=PermissionError), \
                    self.assertRaisesRegex(JourneyError, 'ENFORCEMENT_UNAVAILABLE'):
                enforcement.live_container_contract({'State': {'Pid': 42}}, ['-I'], proc_root=proc, cgroup_root=group)
            source = (selected / 'mountinfo').read_text()
            with self.assertRaisesRegex(JourneyError, 'TMPFS'):
                enforcement.mount_contract(source.replace('rw,nosuid,nodev', 'rw,nosuid,nodev,noexec'))
            with self.assertRaisesRegex(JourneyError, 'ROOT_FILESYSTEM'):
                enforcement.mount_contract(source.replace('/ / ro', '/ / rw'))
