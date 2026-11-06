"""Offline JSON policy contracts only. Never load a filter or invoke its syscalls."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

import journey_reliability_seccomp as seccomp
from journey_process import JourneyError
from test_journey_reliability import OfflineTests


def decision(policy, name, arguments=()):
    """Small JSON-rule evaluator for these exact rules, not a kernel verifier."""
    for rule in policy['syscalls']:
        if name not in rule['names']:
            continue
        matched = True
        for condition in rule.get('args', []):
            actual = arguments[condition['index']]
            if condition['op'] == 'SCMP_CMP_EQ':
                matched &= actual == condition['value']
            elif condition['op'] == 'SCMP_CMP_MASKED_EQ':
                matched &= actual & condition['value'] == condition['valueTwo']
            else:
                raise AssertionError('unreviewed comparator')
        if matched:
            return rule['action'], rule.get('errnoRet')
    return policy['defaultAction'], policy['defaultErrnoRet']


class SeccompContractTests(OfflineTests):
    def test_exact_source_pin_native_architecture_and_default_deny(self):
        raw = seccomp.pinned_profile_bytes()
        self.assertEqual(hashlib.sha256(raw).hexdigest(), seccomp.PROFILE_SHA256)
        profile = json.loads(raw)
        self.assertEqual(profile['architectures'], ['SCMP_ARCH_X86_64'])
        self.assertNotIn('archMap', profile)
        self.assertEqual(profile['defaultAction'], 'SCMP_ACT_ERRNO')
        self.assertEqual(profile['defaultErrnoRet'], 1)
        self.assertEqual(len(seccomp.BASE_SYSCALLS), len(set(seccomp.BASE_SYSCALLS)))
        for rule in profile['syscalls']:
            self.assertFalse({'includes', 'excludes', 'listenerPath', 'listenerMetadata'} & set(rule))
        with patch.object(seccomp, 'PROFILE_SHA256', '0' * 64), self.assertRaisesRegex(JourneyError, 'SOURCE_PIN'):
            seccomp.pinned_profile_bytes()

    def test_only_local_unix_ipv4_ipv6_socket_domains_allowed(self):
        policy = seccomp.profile()
        for constructor in ('socket', 'socketpair'):
            for family in range(0, 256):
                with self.subTest(constructor=constructor, family=family):
                    expected = 'SCMP_ACT_ALLOW' if family in (1, 2, 10) else 'SCMP_ACT_ERRNO'
                    self.assertEqual(decision(policy, constructor, (family, 1, 0))[0], expected)
        # Explicit regressions: NETLINK, PACKET, ALG, VSOCK, XDP and unknown family.
        for family in (16, 17, 38, 40, 44, 65535, -1):
            self.assertEqual(decision(policy, 'socket', (family, 1, 0))[0], 'SCMP_ACT_ERRNO')

    def test_indirect_socket_compatibility_and_escape_mechanisms_are_denied(self):
        forbidden = '''socketcall io_uring_setup io_uring_enter io_uring_register
        pidfd_getfd ptrace process_vm_readv process_vm_writev keyctl add_key request_key
        setns unshare mount umount2 pivot_root chroot open_by_handle_at name_to_handle_at
        bpf perf_event_open userfaultfd mknod mknodat init_module finit_module delete_module
        reboot ioperm iopl kexec_load fsopen fsmount move_mount open_tree mount_setattr
        modify_ldt set_thread_area seccomp unknown_future_syscall'''.split()
        for syscall in forbidden:
            with self.subTest(syscall=syscall):
                self.assertEqual(decision(seccomp.profile(), syscall, (0,) * 6)[0], 'SCMP_ACT_ERRNO')

    def test_clone_is_constrained_and_clone3_fallback_cannot_create_namespaces(self):
        policy = seccomp.profile()
        self.assertEqual(decision(policy, 'clone', (17,))[0], 'SCMP_ACT_ALLOW')
        self.assertEqual(decision(policy, 'clone', (0x003d0f00,))[0], 'SCMP_ACT_ALLOW')
        for flag in (0x00020000, 0x02000000, 0x04000000, 0x08000000,
                     0x10000000, 0x20000000, 0x40000000, 0x80):
            with self.subTest(flag=flag):
                self.assertEqual(decision(policy, 'clone', (17 | flag,))[0], 'SCMP_ACT_ERRNO')
        self.assertEqual(decision(policy, 'clone3', (0,) * 6), ('SCMP_ACT_ERRNO', 38))

    def test_daemon_must_return_full_exact_profile_not_an_asserted_path_or_name(self):
        prefix = ['no-new-privileges']
        self.assertEqual(seccomp.security_options_contract(prefix + ['seccomp=' + seccomp.profile_bytes().decode()]),
                         seccomp.PROFILE_SHA256)
        for token in ('seccomp=unconfined', 'seccomp=builtin', 'seccomp=/owned/profile.json', 'seccomp={}'):
            with self.subTest(token=token), self.assertRaises(JourneyError):
                seccomp.security_options_contract(prefix + [token])
        for mutation in ('allow-default', 'compat', 'socketcall', 'vsock', 'unfiltered-clone'):
            value = seccomp.profile()
            if mutation == 'allow-default':
                value['defaultAction'] = 'SCMP_ACT_ALLOW'
            elif mutation == 'compat':
                value['architectures'].append('SCMP_ARCH_X86')
            elif mutation == 'socketcall':
                value['syscalls'][0]['names'].append('socketcall')
            elif mutation == 'vsock':
                value['syscalls'][1]['args'][0]['value'] = 40
            else:
                value['syscalls'][-2].pop('args')
            with self.subTest(mutation=mutation), self.assertRaisesRegex(JourneyError, 'PROFILE'):
                seccomp.security_options_contract(prefix + ['seccomp=' + json.dumps(value)])
        with self.assertRaises(JourneyError):
            seccomp.security_options_contract(prefix + ['seccomp=' + seccomp.profile_bytes().decode()] * 2)

    def test_private_profile_file_is_generated_from_pin_without_running_anything(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'profile.json'
            seccomp.write_profile(path)
            self.assertEqual(path.read_bytes(), seccomp.pinned_profile_bytes())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
