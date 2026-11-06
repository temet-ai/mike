"""Opt-in, local-image normal-default adapter. No Docker action runs at import."""
import re
import tempfile
from pathlib import Path
import uuid

from journey import new_result
from journey_process import JourneyError, require
from journey_reliability import reliability_policy
from journey_reliability_evidence import sanitize_result
from journey_reliability_native_runtime import selected_runtime
from journey_reliability_enforcement import daemon_contract, live_container_contract
from journey_reliability_payload import (BOOTSTRAP, MAX_RESULT_BYTES, WAIT_RESULT,
                                         prepare_payload, result_from_tar)
from journey_reliability_profiles import NORMAL_PROFILE, reliability_options
from journey_reliability_runner_contract import (container_contract, create_arguments, image_contract,
                                                 owns_container)
from journey_reliability_transport import CONTAINER_ID, IMAGE_REF, local_docker

LIMITS = [
    'Opt-in trusted local Linux/amd64 image and selected local Docker Unix endpoint only; no pulls or host mounts.',
    'Actual daemon-inspected namespace configuration is admission evidence; inner workload assertions cannot grant isolation.',
    'Private container includes candidate, PostgreSQL 19 and synthetic loopback fixtures; no provider or real credentials.',
    'Synthetic service latency is not real-provider performance; browser, baseline comparison and full AC20 remain NOT_RUN.',
    'Bounded allowlisted evidence only; raw logs, captures, database files and container filesystem are never exported.',
    'All AC01-AC27 remain required NOT_RUN; offline adapter tests are not candidate execution or security qualification.',
]


def cleanup_container(docker, identity, name, owner, image_id):
    # Never sweep labels/names, prune, stop an adopted container, or remove an
    # object whose exact random owner label, image and immutable ID are unclear.
    try:
        if not isinstance(identity, str) or not CONTAINER_ID.fullmatch(identity):
            identity = None
        if identity is None:
            inspected = docker.inspect('container', name)
            identity = inspected.get('Id')
        else:
            inspected = docker.inspect('container', identity)
        owns_container(inspected, identity, name, owner, image_id)
        raw = docker.command(['container', 'rm', '--force', identity], timeout=30, limit=128)
        require(raw.decode('ascii').strip() == identity, 'ISOLATED_CLEANUP_ACKNOWLEDGMENT')
        return {'state': 'PASSED', 'exact_owned_container_removed': True, 'container_id': identity}
    except Exception:
        return {'state': 'FAILED', 'exact_owned_container_removed': False,
                'retained_container_id': identity if isinstance(identity, str) and CONTAINER_ID.fullmatch(identity) else None,
                'retained_container_name': name, 'code': 'ISOLATED_CLEANUP_OWNERSHIP_UNCLEAR_OR_UNAVAILABLE'}


def execute_container(docker, snapshot, manifest, digest, request, image, result, runtime_name):
    require(isinstance(image, str) and IMAGE_REF.fullmatch(image), 'ISOLATED_IMAGE_DIGEST_REQUIRED')
    daemon = daemon_contract(docker, runtime_name)
    image_id = image_contract(docker.inspect('image', image), image)
    result['isolation'] = {'state': 'NOT_RUN', 'image_digest': image.split('@')[1],
                           'image_id': image_id, 'platform': 'linux/amd64', **daemon}
    owner = uuid.uuid4().hex
    name = 'mike-release-normal-' + owner
    identity = None
    created = False
    with tempfile.TemporaryDirectory(prefix='mike-runner-payload-') as temporary:
        transfer = Path(temporary) / 'payload.tar'
        payload = prepare_payload(snapshot, manifest, digest, transfer, request)
        result['payload'] = payload
        args = create_arguments(image_id, name, owner, BOOTSTRAP, payload['launch_sha256'], docker.seccomp_profile, runtime_name)
        command = ['-I', '-B', '-c', BOOTSTRAP, payload['launch_sha256']]
        try:
            # A transport timeout may mean create succeeded. Inspect only this
            # random name to recover the immutable ID, and still prove ownership.
            require(daemon_contract(docker, runtime_name) == daemon, 'ISOLATED_DAEMON_PREREQUISITES_CHANGED')
            created = True
            raw = docker.command(args, timeout=30, limit=128)
            identity = raw.decode('ascii').strip()
            require(CONTAINER_ID.fullmatch(identity), 'ISOLATED_CREATE_ID')
            observed = container_contract(docker.inspect('container', identity), identity, name, owner,
                                          image_id, command, runtime_name=runtime_name)
            result['isolation'].update(observed, state='RUNNING', container_id=identity)
            docker.command(['container', 'start', identity], timeout=30, limit=128)
            live = docker.inspect('container', identity)
            container_contract(live, identity, name, owner, image_id, command, running=True, runtime_name=runtime_name)
            result['isolation'].update(live_container_contract(live, command), state='PASSED')
            with transfer.open('rb') as source:
                docker.command(['container', 'cp', '--archive', '-', identity + ':/work'],
                               timeout=180, input_file=source, limit=128)
            timeout = request['duration_seconds'] + 900
            ready = docker.command(['container', 'exec', identity, '/usr/bin/python3', '-I', '-B', '-c',
                                    WAIT_RESULT, str(timeout)], timeout=timeout + 5, limit=128)
            require(ready == b'ready\n', 'ISOLATED_RESULT_READINESS')
            try:
                require(daemon_contract(docker, runtime_name) == daemon, 'ISOLATED_DAEMON_PREREQUISITES_CHANGED')
                live = docker.inspect('container', identity)
                container_contract(live, identity, name, owner, image_id, command, running=True, runtime_name=runtime_name)
                final_boundary = live_container_contract(live, command)
                require(all(result['isolation'].get(key) == value for key, value in final_boundary.items()),
                        'ISOLATED_HOST_BOUNDARY_CHANGED')
            except Exception:
                result['isolation']['state'] = 'FAILED'
                raise
            raw = docker.command(['container', 'cp', identity + ':/work/result.json', '-'],
                                 timeout=30, limit=MAX_RESULT_BYTES + 65536)
            value, report_digest = result_from_tar(raw)
            result.update(sanitize_result(value, manifest, digest, request=request))
            result['inner_report_sha256'] = report_digest
        finally:
            if result['isolation']['state'] != 'PASSED':
                result['isolation']['state'] = 'FAILED'
            if created:
                result['container_cleanup'] = cleanup_container(docker, identity, name, owner, image_id)
                if result['container_cleanup']['state'] != 'PASSED':
                    result.update(state='FAILED', failure_code='ISOLATED_CONTAINER_CLEANUP_FAILED')


def run_isolated_normal_journey(snapshot, manifest, manifest_sha256, *, duration_seconds=None,
                                concurrent_sessions=None, resource_budgets, docker_binary,
                                docker_endpoint, runner_image, runtime_name):
    """Only explicit runtime entry. Offline contracts never invoke this unmocked."""
    result = new_result(None)
    result.update(runner='isolated-normal-default-reliability-helper', limits=list(LIMITS))
    try:
        selected_runtime(runtime_name)
        policy = reliability_policy(duration_seconds, concurrent_sessions)
        options = reliability_options(NORMAL_PROFILE, resource_budgets)
        require(options['resource_budgets']['active_streams_peak'] >= policy['concurrent_sessions'],
                'RELIABILITY_ACTIVE_STREAMS_BUDGET_BELOW_WORKLOAD')
        require(manifest.get('target') == 'x86_64-unknown-linux-gnu', 'ISOLATED_CANDIDATE_PLATFORM')
        request = {key: policy[key] for key in ('duration_seconds', 'concurrent_sessions')}
        request['resource_budgets'] = options['resource_budgets']
        result['requested_policy'] = dict(request)
        result['runtime_alias'] = runtime_name
        with local_docker(docker_binary, docker_endpoint) as docker:
            execute_container(docker, Path(snapshot), manifest, manifest_sha256, request, runner_image, result, runtime_name)
    except Exception as error:
        result.update(state='FAILED', failure_type=type(error).__name__)
        if isinstance(error, JourneyError) and re.fullmatch(r'[A-Z][A-Z0-9_]{0,127}', str(error)):
            result['failure_code'] = str(error)
    return result
