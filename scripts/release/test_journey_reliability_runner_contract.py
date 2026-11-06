"""Offline isolation configuration contracts; all socket/process APIs are blocked."""
import copy
from pathlib import Path
import tempfile
from unittest.mock import patch

import journey_reliability_runner_contract as contract
import journey_reliability_transport as transport
from journey_process import JourneyError
from journey_reliability_seccomp import pinned_profile_bytes
from test_journey_reliability import OfflineTests

IMAGE = 'trusted.invalid/runner@sha256:' + 'a' * 64
IMAGE_ID = 'sha256:' + 'b' * 64
CID = 'c' * 64
OWNER = 'd' * 32
NAME = 'mike-release-normal-' + OWNER
COMMAND = ['-I', '-B', '-c', 'bootstrap', 'e' * 64]


def image():
    return {'Id': IMAGE_ID, 'RepoDigests': [IMAGE], 'Os': 'linux', 'Architecture': 'amd64',
            'Config': {'Env': ['PATH=/usr/bin:/bin']}}


def container(running=False):
    return {'Id': CID, 'Name': '/' + NAME, 'Image': IMAGE_ID,
            'Config': {'Labels': {contract.OWNER_LABEL: OWNER, contract.CONTRACT_LABEL: contract.CONTRACT_VERSION},
                       'User': f'{contract.UID}:{contract.GID}', 'WorkingDir': '/work', 'Entrypoint': ['/usr/bin/python3'],
                       'Cmd': COMMAND, 'Hostname': 'mike-release-runner', 'Image': IMAGE_ID,
                       'Env': [key + '=' + value for key, value in contract.ENVIRONMENT.items()],
                       'Healthcheck': {'Test': ['NONE']}, 'Tty': False, 'OpenStdin': False},
            'HostConfig': {'NetworkMode': 'none', 'Runtime': 'mike-release-native', 'ReadonlyRootfs': True, 'Privileged': False,
                           'CapDrop': ['ALL'], 'SecurityOpt': ['no-new-privileges', 'seccomp=' + pinned_profile_bytes().decode()], 'IpcMode': 'private',
                           'CgroupnsMode': 'private', 'Memory': contract.MEMORY_BYTES,
                           'MemorySwap': contract.MEMORY_BYTES, 'NanoCpus': 2_000_000_000,
                           'PidsLimit': 256, 'ShmSize': 64 * 1024 ** 2, 'Tmpfs': {'/work': contract.TMPFS},
                           'PublishAllPorts': False, 'AutoRemove': False,
                           'RestartPolicy': {'Name': 'no'}, 'LogConfig': {'Type': 'none'}},
            'Mounts': [], 'NetworkSettings': {'Networks': {'none': {}}},
            'State': {'Running': running, 'Paused': False, 'Restarting': False,
                      'Status': 'running' if running else 'created'}}


class ImageContractTests(OfflineTests):
    def test_exact_local_digest_and_platform_required(self):
        self.assertEqual(contract.image_contract(image(), IMAGE), IMAGE_ID)
        invalid = [('Os', 'windows'), ('Architecture', 'arm64'), ('Variant', 'v7'),
                   ('Id', 'latest'), ('RepoDigests', ['different@sha256:' + 'a' * 64])]
        for key, value in invalid:
            changed = image(); changed[key] = value
            with self.subTest(key=key), self.assertRaises(JourneyError):
                contract.image_contract(changed, IMAGE)
        for value in ('runner:latest', 'sha256:' + 'a' * 64, 'runner@sha256:no', 'https://runner@sha256:' + 'a' * 64):
            with self.subTest(value=value), self.assertRaises(JourneyError):
                contract.image_contract(image(), value)

    def test_image_secret_environment_volumes_and_duplicate_keys_rejected(self):
        for config in ({'Env': ['TOKEN=private']}, {'Volumes': {'/data': {}}},
                       {'Env': ['PATH=a', 'PATH=b']}, {'Env': ['MALFORMED']}):
            changed = image(); changed['Config'] = config
            with self.subTest(config=config), self.assertRaises(JourneyError):
                contract.image_contract(changed, IMAGE)


class ContainerContractTests(OfflineTests):
    def test_daemon_observed_minimal_private_configuration_passes(self):
        for running in (False, True):
            result = contract.container_contract(container(running), CID, NAME, OWNER, IMAGE_ID, COMMAND, running=running, runtime_name="mike-release-native")
            self.assertTrue(result['daemon_observed'])
            self.assertEqual(result['network_mode'], 'none')
            self.assertFalse(result['host_mounts'])

    def test_each_escalation_or_different_owner_fails_closed(self):
        mutations = [('HostConfig', 'NetworkMode', 'host'), ('HostConfig', 'Privileged', True),
            ('HostConfig', 'ReadonlyRootfs', False), ('HostConfig', 'CapDrop', []), ('HostConfig', 'CapAdd', ['SYS_ADMIN']),
            ('HostConfig', 'SecurityOpt', ['seccomp=unconfined']), ('HostConfig', 'PidMode', 'host'),
            ('HostConfig', 'IpcMode', 'host'), ('HostConfig', 'UTSMode', 'host'),
            ('HostConfig', 'UsernsMode', 'host'), ('HostConfig', 'CgroupnsMode', 'host'),
            ('HostConfig', 'Runtime', 'untrusted'), ('HostConfig', 'Runtime', ''), ('HostConfig', 'Memory', 0), ('HostConfig', 'MemorySwap', -1),
            ('HostConfig', 'PidsLimit', -1), ('HostConfig', 'NanoCpus', 0),
            ('HostConfig', 'Tmpfs', {'/work': 'rw,noexec'}), ('HostConfig', 'Binds', ['/host:/host']),
            ('HostConfig', 'Mounts', [{'Source': '/var/run/docker.sock'}]), ('HostConfig', 'Devices', ['/dev/kvm']),
            ('HostConfig', 'PortBindings', {'80/tcp': [{}]}), ('HostConfig', 'PublishAllPorts', True),
            ('HostConfig', 'AutoRemove', True), ('HostConfig', 'RestartPolicy', {'Name': 'always'}),
            ('Config', 'User', '0'), ('Config', 'Env', ['SECRET=private']), ('Config', 'OpenStdin', True),
            ('Config', 'Entrypoint', ['/bin/sh']), ('Config', 'Healthcheck', {'Test': ['CMD', 'unsafe']}),
            ('State', 'Running', True), ('State', 'Paused', True)]
        for group, key, value in mutations:
            changed = container(); changed[group][key] = value
            with self.subTest(group=group, key=key), self.assertRaises(JourneyError):
                contract.container_contract(changed, CID, NAME, OWNER, IMAGE_ID, COMMAND, runtime_name="mike-release-native")
        for key, value in [('Id', 'f' * 64), ('Image', 'sha256:' + 'f' * 64), ('Name', '/other'),
                           ('Mounts', [{'Type': 'bind', 'Destination': '/work'}])]:
            changed = container(); changed[key] = value
            with self.subTest(key=key), self.assertRaises(JourneyError):
                contract.container_contract(changed, CID, NAME, OWNER, IMAGE_ID, COMMAND, runtime_name="mike-release-native")
        changed = container(); changed['Config']['Labels'][contract.OWNER_LABEL] = 'foreign'
        with self.assertRaises(JourneyError):
            contract.owns_container(changed, CID, NAME, OWNER, IMAGE_ID)

    def test_created_arguments_cannot_publish_mount_pull_or_use_shell(self):
        args = contract.create_arguments(IMAGE_ID, NAME, OWNER, 'bootstrap', 'a' * 64, Path('/owned/seccomp.json'), 'mike-release-native')
        self.assertIn('--pull=never', args)
        self.assertEqual(args[args.index('--runtime') + 1], 'mike-release-native')
        self.assertEqual(args[args.index('--network') + 1], 'none')
        self.assertIn('exec', args[args.index('--tmpfs') + 1].split(','))
        for forbidden in ('--privileged', '--mount', '--volume', '--publish', '--pid', '/bin/sh'):
            self.assertNotIn(forbidden, args)
        self.assertEqual(args[-6:], [IMAGE_ID, '-I', '-B', '-c', 'bootstrap', 'a' * 64])


class ExplicitTransportTests(OfflineTests):
    def test_clean_environment_and_endpoint_identity_pinned_without_opening_socket(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(transport.platform, 'system', return_value='Linux'), \
                patch.object(transport.platform, 'machine', return_value='x86_64'), \
                patch.object(transport, 'plain_absolute', side_effect=lambda path, **kwargs: (Path(path), (1, 2))), \
                patch.object(transport, 'verify_peer', return_value={'verified': True}), \
                patch.object(transport, 'bounded_command', return_value=b'[]') as command:
            docker = transport.LocalDocker('/usr/bin/docker', '/run/docker.sock', Path(temporary))
            docker.command(['image', 'inspect', IMAGE])
            args = command.call_args.args[0]
            self.assertIn('unix:///run/docker.sock', args)
            self.assertEqual(command.call_args.kwargs['environment']['PATH'], '')
            self.assertFalse(any(key.startswith('DOCKER_') for key in command.call_args.kwargs['environment']))
            self.assertEqual((docker.config / 'config.json').read_text(), '{}\n')
            with patch.object(transport, 'plain_absolute', return_value=(Path('/changed'), (9, 9))), self.assertRaises(JourneyError):
                docker.command(['container', 'start', CID])
            self.assertEqual(command.call_count, 1)
