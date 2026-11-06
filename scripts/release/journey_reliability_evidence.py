"""Closed projection of container evidence: no logs, paths, URLs or arbitrary text."""
import math
import re

from journey_process import require
from journey_reliability_isolation import FEATURES
from journey_reliability_profiles import BUDGET_FIELDS, NORMAL_ENABLED, NORMAL_PROFILE
from journey_reliability_defaults import ENDPOINT_OVERRIDES

STATES = {'NOT_RUN', 'RUNNING', 'PASSED', 'FAILED', 'INCOMPLETE'}
COUNTERS = set('''feature_observations requested_duration_seconds minimum_duration_seconds duration_seconds
requested_concurrent_sessions peak_concurrent_sessions minimum_batch_concurrency attempted_requests
completed_client_requests durably_accounted_requests attempted accepted rejected request_bytes response_bytes completed_batches health_observations synthetic_request_bytes
synthetic_response_bytes failed_run_count samples active_streams_peak broadcast_backlog_sampled_peak drain_checks
mcp_peak_seconds spa_http_peak_seconds sample_interval_seconds rss_sampled_peak_bytes rss_lifetime_peak_bytes
fd_sampled_peak rss_current_delta_bytes fd_delta rss_current_bytes fd_count rss_peak_bytes fd_peak
broadcast_backlog_peak drain_seconds responsiveness_seconds requests complete sessions orphaned tool_uses
migration_count failed_rows harvested_lessons registered_workspaces registered_sessions
settings_observations effective_settings_count serialized_boot_fields endpoint_relocations registrations
payload_bytes finite_requests blocked_requests observed_requests expected_requests bytes file_count nonroot_uid
'''.split()) | set(BUDGET_FIELDS)
BOOLEANS = set('''duration_policy_met concurrency_policy_met observed_values_within_explicit_budgets
exact_durable_ids_and_sessions exact_durable_tool_wire_ids exact_candidate_set owned_pid_and_generation
persistent_enabled_and_reachable embedding_disabled embedding_url_is_compiled_default openrouter_key_absent
embedding_endpoint_owned independent_owned_database maintenance_capture_untouched
ledger_unchanged private_files_unchanged exact_installed_hashes executable_source_identity
candidate_effective_defaults_verified feature_api_mutations_allowed owned_cluster_stopped
inner_boundary_observed outer_daemon_inspection_required distinct_host_namespaces only_loopback_interface
root_filesystem_readonly capabilities_empty no_new_privileges seccomp_filter_present compatible_tools_observed
owned_process_group_only bounded_cleanup owned_loopback_fixture_only only_databases_created_by_this_run
supported_api_only owned_synthetic_location normal_defaults_observed durable_synthetic_worker_effects
request_limit_exceeded synthetic_seed_used empty_before_candidate new_owned_database compiled_boot_defaults_preserved owned_project_and_location_durable
exact_registration_and_lesson_provenance roster_api_matches harvested_lesson_api_matches
'''.split())
HASHES = set('''full_ledger_sha256 candidate_inventory_sha256 capture_set_sha256
settings_snapshot_sha256 database_defaults_sha256 boot_snapshot_sha256 worker_rows_sha256
request_body_sha256 response_body_sha256 fixture_transcript_sha256 workload_sha256 accounting_sha256
'''.split())
GROUPS = {'resources', 'product_observations', 'responsiveness', 'baseline', 'final', 'resource_budgets',
          'worker_observations', 'worker_exercise', 'capture_accounting', 'finite_service', 'services',
          'harvester', 'workspace_registration', 'workload', 'observation', 'boot_defaults',
          'post_stop_worker_observations', 'service_observations'}
UNMEASURED = {'normal_default_reliability', 'os_rss', 'os_file_descriptors', 'product_queue_depth',
              'browser_responsiveness', 'mcp_responsiveness', 'spa_http_responsiveness', 'fault_injection',
              'external_provider', 'baseline_comparison'}


def projection(value, depth=0):
    require(isinstance(value, dict) and depth <= 8 and len(value) <= 128, 'ISOLATED_EVIDENCE_SHAPE')
    output = {}
    for key, item in value.items():
        if key in COUNTERS:
            require(type(item) in (int, float) and math.isfinite(item) and abs(item) <= 2 ** 53,
                    'ISOLATED_EVIDENCE_NUMBER')
            output[key] = item
        elif key in BOOLEANS:
            require(type(item) is bool, 'ISOLATED_EVIDENCE_BOOLEAN')
            output[key] = item
        elif key in HASHES:
            require(isinstance(item, str) and re.fullmatch('[0-9a-f]{64}', item), 'ISOLATED_EVIDENCE_HASH')
            output[key] = item
        elif key == 'state' or key == 'workload_state':
            require(item in STATES, 'ISOLATED_EVIDENCE_STATE')
            output[key] = item
        elif key == 'worker_exercise':
            require(isinstance(item, dict) and set(item) <= {'workspace_capture_registration', 'harvester',
                    'fleet_auto_start', 'prompt_assistant', 'background_worker_internal_cycles',
                    'embedding_consolidation'} and all(state in STATES for state in item.values()),
                    'ISOLATED_EVIDENCE_WORKER_STATES')
            output[key] = dict(item)
        elif key in GROUPS and isinstance(item, dict):
            output[key] = projection(item, depth + 1)
        elif key == 'effective_feature_flags' and item is not None:
            require(isinstance(item, dict) and set(item) == set(FEATURES)
                    and all(type(flag) is bool for flag in item.values()), 'ISOLATED_EVIDENCE_FEATURES')
            output[key] = dict(item)
        elif key == 'unmeasured':
            require(isinstance(item, dict) and all(state == 'NOT_RUN' for state in item.values()),
                    'ISOLATED_EVIDENCE_UNMEASURED')
            output[key] = {name: 'NOT_RUN' for name in UNMEASURED if name in item}
        elif key in ('python_version', 'postgres_version'):
            require(isinstance(item, str) and re.fullmatch(r'[0-9]+\.[0-9]+(?:\.[0-9]+)?', item),
                    'ISOLATED_EVIDENCE_VERSION')
            output[key] = item
    return output


def expected_binding(manifest, digest):
    artifacts = []
    for item in manifest['artifacts']:
        binary = [entry for entry in item['contents'] if entry['path'] == item['binary']]
        require(len(binary) == 1, 'ISOLATED_BINDING_INVENTORY')
        artifacts.append({'binary': item['binary'], 'binary_sha256': binary[0]['sha256'],
                          'archive_sha256': item['sha256']})
    return {'manifest_sha256': digest, 'source_commit': manifest['source_commit'],
            'version': manifest['version'], 'target': manifest['target'], 'artifacts': artifacts}


def validate_policy(value, request, complete):
    require(isinstance(value, dict)
            and value.get('requested_duration_seconds') == request['duration_seconds']
            and value.get('requested_concurrent_sessions') == request['concurrent_sessions']
            and value.get('resource_budgets') == request['resource_budgets']
            and value.get('minimum_duration_seconds') == 900,
            'ISOLATED_RESULT_POLICY_MISMATCH')
    require(value.get('requested_disabled_feature_keys') == []
            and value.get('inner_isolation_verified', False) is False, 'ISOLATED_RESULT_INNER_AUTHORITY')
    if not complete:
        return
    require(value.get('workload_state') == 'PASSED' and value.get('duration_policy_met') is True
            and value.get('concurrency_policy_met') is True
            and type(value.get('duration_seconds')) in (int, float)
            and request['duration_seconds'] <= value['duration_seconds'] <= request['duration_seconds'] + 120
            and value.get('minimum_batch_concurrency', 0) >= request['concurrent_sessions'],
            'ISOLATED_RESULT_WORKLOAD_INCOMPLETE')
    require(value.get('effective_feature_flags') == {key: key in NORMAL_ENABLED for key in FEATURES}
            and value.get('candidate_effective_defaults_verified') is True
            and value.get('override_names') == list(ENDPOINT_OVERRIDES), 'ISOLATED_RESULT_DEFAULTS')
    count = value.get('attempted_requests')
    require(type(count) is int and request['concurrent_sessions'] <= count <= 25000
            and count == value.get('completed_client_requests') == value.get('durably_accounted_requests')
            and value.get('completed_batches', 0) > 0, 'ISOLATED_RESULT_ACCOUNTING')
    for key in ('resources', 'product_observations', 'responsiveness'):
        require(isinstance(value.get(key), dict) and value[key].get('state') == 'PASSED'
                and value[key].get('samples', 0) > 0, 'ISOLATED_RESULT_RESOURCE_OBSERVATIONS')
    workers = value.get('worker_exercise', {})
    require(isinstance(workers, dict) and workers.get('workspace_capture_registration') == 'PASSED'
            and workers.get('harvester') == 'PASSED', 'ISOLATED_RESULT_WORKER_EXERCISE')


def sanitize_result(value, manifest, digest, *, request=None):
    require(value.get('qualified') is False and value.get('state') in ('FAILED', 'INCOMPLETE'),
            'ISOLATED_RESULT_QUALIFICATION')
    cases = [{'id': f'AC{number:02}', 'state': 'NOT_RUN', 'required': True} for number in range(1, 28)]
    require(value.get('full_cases', cases) == cases, 'ISOLATED_RESULT_CASE_QUALIFICATION')
    binding = expected_binding(manifest, digest)
    require(value.get('binding') == binding or value.get('state') == 'FAILED' and value.get('binding') is None,
            'ISOLATED_RESULT_BINDING')
    result = {'state': value['state'], 'qualified': False, 'binding': value.get('binding'),
              'full_cases': cases, 'checks': []}
    checks = value.get('checks')
    require(isinstance(checks, list) and len(checks) <= 128, 'ISOLATED_RESULT_CHECKS')
    for item in checks:
        require(isinstance(item, dict) and isinstance(item.get('id'), str)
                and re.fullmatch(r'archive_(?:normal|reliability)_[a-z0-9_]{1,100}', item['id'])
                and item.get('state') in STATES, 'ISOLATED_RESULT_CHECK')
        check = {'id': item['id'], 'state': item['state'], 'binding': value.get('binding')}
        if isinstance(item.get('observation'), dict):
            check['observation'] = projection(item['observation'])
        if 'code' in item:
            require(isinstance(item['code'], str) and re.fullmatch(r'[A-Z][A-Z0-9_]{0,127}', item['code']),
                    'ISOLATED_RESULT_CODE')
            check['code'] = item['code']
        result['checks'].append(check)
    if 'failure_code' in value:
        require(isinstance(value['failure_code'], str) and re.fullmatch(r'[A-Z][A-Z0-9_]{0,127}', value['failure_code']),
                'ISOLATED_RESULT_CODE')
        result['failure_code'] = value['failure_code']
    require(value['state'] == 'FAILED' or isinstance(value.get('reliability'), dict), 'ISOLATED_RESULT_WORKLOAD_MISSING')
    if 'reliability' in value:
        if request is not None:
            validate_policy(value['reliability'], request, value['state'] == 'INCOMPLETE')
        require(value['reliability'].get('profile') == NORMAL_PROFILE, 'ISOLATED_RESULT_PROFILE')
        result['reliability'] = projection(value['reliability'])
        result['reliability'].update(profile=NORMAL_PROFILE, feature_api_mutations_allowed=False,
                                     duration_clock='time.monotonic')
        if value['state'] == 'INCOMPLETE':
            require(bool(checks) and all(item['state'] == 'PASSED' for item in checks), 'ISOLATED_RESULT_PARTIAL_CHECKS')
    return result
