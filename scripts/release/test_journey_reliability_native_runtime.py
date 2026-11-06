"""Offline bounded ELF identity contracts; synthetic fixture bytes never execute."""
import copy
from pathlib import Path
import struct
import stat
from types import SimpleNamespace
from unittest.mock import patch
import tempfile

import journey_reliability_native_runtime as runtime
from journey_process import JourneyError
from test_journey_reliability import OfflineTests


def header():
    value = bytearray(64)
    value[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<H', value, 16, 3)
    struct.pack_into('<H', value, 18, 62)
    struct.pack_into('<I', value, 20, 1)
    struct.pack_into('<H', value, 52, 64)
    return bytes(value)


def info(path):
    return {'Runtimes': {'mike-release-native': {'path': str(path)}}}


class NativeRuntimeTests(OfflineTests):
    def setUp(self):
        super().setUp()
        # Synthetic temp files are deliberately unprivileged. Test root-ownership
        # policy separately, then exercise bounded reading with modeled ownership.
        self.actual_root_owned = runtime.root_owned
        self.enterContext(patch.object(runtime, 'root_owned'))

    def test_root_owned_runtime_and_every_parent_must_not_be_shared_writable(self):
        good = SimpleNamespace(st_uid=0, st_mode=stat.S_IFREG | 0o755)
        self.actual_root_owned(good)
        for uid, mode, directory in ((1000, stat.S_IFREG | 0o755, False),
                                     (0, stat.S_IFREG | 0o775, False),
                                     (0, stat.S_IFDIR | 0o777, True),
                                     (1000, stat.S_IFDIR | 0o755, True)):
            with self.subTest(uid=uid, mode=mode), self.assertRaisesRegex(JourneyError, 'ROOT_OWNERSHIP'):
                self.actual_root_owned(SimpleNamespace(st_uid=uid, st_mode=mode), directory=directory)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'runc'; path.write_bytes(header()); path.chmod(0o700)
            with patch.object(runtime, 'root_owned') as ownership:
                runtime.trusted_path(path)
            self.assertEqual(ownership.call_count, len(path.parents) + 1)

    def test_daemon_path_must_be_explicit_absolute_without_custom_wrapper_options(self):
        self.assertEqual(runtime.runtime_path(info('/usr/bin/runc'), 'mike-release-native'), Path('/usr/bin/runc'))
        values = ({}, {'Runtimes': {}}, info('runc'), info('./runc'), info('/usr/../bin/runc'))
        for value in values:
            with self.subTest(value=value), self.assertRaisesRegex(JourneyError, 'EXPLICIT_PATH_REQUIRED'):
                runtime.runtime_path(value, 'mike-release-native')
        for field, value in (('runtimeArgs', ['--root=/private']), ('runtimeType', 'custom'), ('options', {'BinaryName': 'other'})):
            selected = info('/usr/bin/runc'); selected['Runtimes']['mike-release-native'][field] = value
            with self.subTest(field=field), self.assertRaises(JourneyError):
                runtime.runtime_path(selected, 'mike-release-native')

    def test_only_native_elf64_little_endian_x86_64_header_is_accepted(self):
        runtime.native_elf(header())
        for offset, value in ((4, 1), (5, 2), (6, 0), (18, 3), (52, 0)):
            changed = bytearray(header()); changed[offset] = value
            with self.subTest(offset=offset), self.assertRaisesRegex(JourneyError, 'NATIVE_AMD64_ELF'):
                runtime.native_elf(bytes(changed))
        for value in (b'#!/bin/sh\n', b'not ELF', header()[:-1]):
            with self.assertRaises(JourneyError):
                runtime.native_elf(value)

    def test_bounded_fd_hash_and_full_identity_are_rechecked(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'runc'; path.write_bytes(header() + b'synthetic inert body'); path.chmod(0o700)
            observed = runtime.verify_runtime(info(path), 'mike-release-native')
            self.assertEqual(runtime.verify_runtime(info(path), 'mike-release-native', observed), observed)
            self.assertEqual(runtime.runtime_evidence(observed)['runtime_elf_machine'], 62)
            replacement = path.with_name('replacement'); replacement.write_bytes(path.read_bytes()); replacement.chmod(0o700)
            replacement.replace(path)
            with self.assertRaisesRegex(JourneyError, 'IDENTITY_CHANGED'):
                runtime.verify_runtime(info(path), 'mike-release-native', observed)
            changed = runtime.verify_runtime(info(path), 'mike-release-native')
            path.write_bytes(header() + b'different body')
            with self.assertRaisesRegex(JourneyError, 'IDENTITY_CHANGED'):
                runtime.verify_runtime(info(path), 'mike-release-native', changed)

    def test_symlink_writable_missing_non_executable_or_oversized_runtime_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); path = root / 'runc'; path.write_bytes(header()); path.chmod(0o700)
            link = root / 'linked'; link.symlink_to(path)
            with self.assertRaisesRegex(JourneyError, 'PATH_SYMLINK'):
                runtime.verify_runtime(info(link), 'mike-release-native')
            with self.assertRaisesRegex(JourneyError, 'LOCAL_IDENTITY_UNAVAILABLE'):
                runtime.verify_runtime(info(root / 'missing'), 'mike-release-native')
            for mode in (0o600, 0o777):
                path.chmod(mode)
                with self.subTest(mode=mode), self.assertRaisesRegex(JourneyError, 'TRUSTED_EXECUTABLE_REQUIRED'):
                    runtime.verify_runtime(info(path), 'mike-release-native')
            path.chmod(0o700)
            with path.open('wb') as output:
                output.truncate(runtime.MAX_RUNTIME_BYTES + 1)
            with self.assertRaisesRegex(JourneyError, 'TRUSTED_EXECUTABLE_REQUIRED'):
                runtime.verify_runtime(info(path), 'mike-release-native')
