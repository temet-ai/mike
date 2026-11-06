"""Offline Unix-peer contracts; socket objects and host process data are mocked."""
from contextlib import contextmanager
from pathlib import Path
import socket
import stat
import struct
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, Mock, patch

import journey_reliability_peer as peer
from journey_process import JourneyError
from test_journey_reliability import OfflineTests


@unittest.skipUnless(hasattr(socket, 'SO_PEERCRED'), 'the direct-daemon peer check needs Linux peer credentials and /proc')
class PeerTests(OfflineTests):
    @contextmanager
    def host(self, *, credentials=(42, 0, 0), namespace='mnt:[1]', root_inode=(1, 2), executable='/usr/bin/dockerd'):
        endpoint = Mock()
        endpoint.__str__ = Mock(return_value='/run/docker.sock')
        endpoint.lstat.return_value = SimpleNamespace(st_mode=stat.S_IFSOCK | 0o660, st_uid=0, st_dev=1, st_ino=3)
        connection = MagicMock()
        connection.__enter__.return_value = connection
        connection.getsockopt.return_value = struct.pack('3i', *credentials)
        def readlink(path):
            path = str(path)
            if path.endswith('/exe'):
                return executable
            return 'mnt:[1]' if path == '/proc/self/ns/mnt' else namespace
        def inode(path):
            if str(path).endswith('/exe'):
                return (8, 9)
            return (1, 2) if str(path) == '/' else root_inode
        binary = {'path': Path(executable), 'metadata': (8, 9, 64), 'sha256': 'a' * 64}
        with patch.object(peer.socket, 'socket', return_value=connection) as constructor, \
                patch.object(peer, 'start_ticks', return_value=123) as ticks, \
                patch.object(peer.os, 'readlink', side_effect=readlink), \
                patch.object(peer, 'inode', side_effect=inode), \
                patch.object(peer, 'observe_runtime', return_value=binary) as runtime:
            yield endpoint, connection, constructor, ticks, runtime

    def test_direct_root_native_daemon_shares_verified_mount_and_root_view(self):
        with self.host() as (endpoint, connection, constructor, _ticks, runtime):
            value = peer.verify_peer(endpoint)
            self.assertEqual(peer.verify_peer(endpoint, value), value)
        constructor.assert_called_with(socket.AF_UNIX, socket.SOCK_STREAM)
        connection.settimeout.assert_called_with(3)
        connection.connect.assert_called_with('/run/docker.sock')
        connection.send.assert_not_called(); connection.sendall.assert_not_called()
        self.assertEqual(runtime.call_args.kwargs['limit'], 256 * 1024 ** 2)
        self.assertEqual(value['pid'], 42)
        self.assertEqual(value['socket_identity'], (1, 3))
        self.assertEqual(connection.__exit__.call_count, 2)

    def test_proxy_systemd_nonroot_mount_or_root_mismatch_rejected(self):
        for options in ({'credentials': (1, 0, 0)}, {'credentials': (42, 1000, 1000)},
                        {'executable': '/usr/bin/socat'}, {'executable': '/usr/bin/dockerd (deleted)'},
                        {'namespace': 'mnt:[2]'}, {'root_inode': (1, 999)}):
            with self.subTest(options=options), self.host(**options) as (endpoint, connection, *_):
                with self.assertRaises(JourneyError):
                    peer.verify_peer(endpoint)
                self.assertEqual(connection.__exit__.call_count, 1)

    def test_identity_drift_or_connection_denial_never_falls_back(self):
        with self.host() as (endpoint, connection, constructor, ticks, _):
            ticks.side_effect = [123, 456]
            with self.assertRaisesRegex(JourneyError, 'PROCESS_CHANGED'):
                peer.verify_peer(endpoint)
        with self.host() as (endpoint, connection, constructor, *_):
            connection.connect.side_effect = PermissionError
            with self.assertRaisesRegex(JourneyError, 'DIRECT_DOCKERD_UNAVAILABLE'):
                peer.verify_peer(endpoint)
            self.assertEqual(constructor.call_count, 1)
            self.assertEqual(connection.__exit__.call_count, 1)
        with self.host() as (endpoint, _connection, *_):
            first = peer.verify_peer(endpoint)
            first['started'] = 999
            with self.assertRaisesRegex(JourneyError, 'PEER_CHANGED'):
                peer.verify_peer(endpoint, first)

    def test_socket_inode_and_running_executable_must_match_pinned_files(self):
        with self.host() as (endpoint, connection, *_):
            initial = endpoint.lstat.return_value
            changed = SimpleNamespace(st_mode=initial.st_mode, st_uid=0, st_dev=1, st_ino=99)
            endpoint.lstat.side_effect = [initial, changed]
            with self.assertRaisesRegex(JourneyError, 'SOCKET_CHANGED'):
                peer.verify_peer(endpoint)
        with self.host() as (endpoint, connection, *_), patch.object(peer, 'inode', return_value=(9, 10)):
            with self.assertRaisesRegex(JourneyError, 'EXECUTABLE_CHANGED'):
                peer.verify_peer(endpoint)
        with self.host() as (endpoint, connection, *_):
            endpoint.lstat.return_value.st_uid = 1000
            with self.assertRaisesRegex(JourneyError, 'ROOT_SOCKET_REQUIRED'):
                peer.verify_peer(endpoint)
            connection.connect.assert_not_called()


@unittest.skipIf(hasattr(socket, 'SO_PEERCRED'), 'covers systems without Linux peer credentials')
class PeerWithoutLinuxCredentialsTests(OfflineTests):
    def test_peer_check_refuses_before_connecting(self):
        endpoint = Mock()
        endpoint.lstat.return_value = SimpleNamespace(st_mode=stat.S_IFSOCK | 0o660, st_uid=0, st_dev=1, st_ino=3)
        with patch.object(peer.socket, 'socket') as factory:
            with self.assertRaisesRegex(JourneyError, 'ISOLATED_LINUX_PEER_CREDENTIALS_REQUIRED'):
                peer.verify_peer(endpoint)
            factory.assert_not_called()
