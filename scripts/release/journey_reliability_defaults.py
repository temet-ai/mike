"""Offline preparation for normal-default reliability; never runtime admission.

These helpers only render configuration and validate supplied observations. They
do not reserve ports, create databases, read operator files, or launch anything.
Loopback addresses and a matching feature response are NOT network isolation.
Direct host execution remains closed; the separate adapter must verify isolation.
"""
import json
import re

from journey_process import require
from journey_reliability_isolation import FEATURES, feature_row

ENDPOINT_ROLES = ("proxy", "inspect", "spa", "upstream", "services", "postgres")
ENDPOINT_OVERRIDES = (
    "proxy_listen_addr", "inspect_listen_addr", "spa_bind", "spa_origins",
    "upstream_url", "database_url", "steerer.openrouter_base_url",
)

# Every direct spawn in serve/workers.rs, including workers without a feature
# toggle. Values describe required fixture conditions, not observed outcomes or
# a claim that the transitive worker implementation has no other dependencies.
WORKER_CONDITIONS = {
    "lesson_worker::spawn_lesson_worker": "owned_database_and_disabled_embedding",
    "resync::worker::spawn_resync_worker": "absent_local_database_url",
    "embed_worker::spawn_embed_backfill": "owned_database_and_disabled_embedding",
    "semantic::spawn_semantic_worker": "owned_database_and_disabled_embedding",
    "judge_worker::spawn_judge_worker": "compiled_feature_disabled",
    "dossier::worker::spawn_dossier_builder": "compiled_feature_disabled",
    "steerer::spawn_steerer_fleet": "compiled_feature_disabled",
    "quiescence_worker::spawn_quiescence_worker": "compiled_feature_disabled",
    "steerer::reply_label::demotion::spawn_steerer_demotion": "owned_database_without_operator_steerers",
    "judge_worker::spawn_session_judge_worker": "compiled_feature_disabled",
    "judge_worker::spawn_session_end_shadow": "compiled_feature_disabled",
    "mailbox_reaper::spawn_mailbox_reaper": "owned_database_without_operator_mail",
    "work_registry_worker::spawn_work_registry_worker": "owned_database_without_operator_work_items_or_file_paths",
    "eval_metrics::spawn_eval_metrics_worker": "owned_database_and_compiled_settings",
    "cross_session::spawn_cross_session_worker": "owned_database_and_compiled_settings",
    "channel_link_sweep::spawn_channel_link_sweep": "owned_database_without_remote_control_channels",
    "proxy::remote_control::spawn_repush_sweep": "no_remote_control_channels_streams_or_mail",
    "workspace_reaper::spawn_workspace_reaper": "synthetic_passive_registrations_without_operator_pids",
    "launch::fleet::display::spawn_fleet_display": "no_fleets_launches_or_retrospective_jobs",
    "change_record::watch::spawn_change_record_watcher": "compiled_feature_disabled",
    "launch::fleet::brief_version::spawn_brief_refresh": "no_fleets_or_launches",
    "launch::cli::spawn_queue_replay": "empty_private_launch_queue",
    "retention_worker::spawn_retention_sweep_worker": "owned_database_and_compiled_retention_settings",
    "api::handlers::spawn_health_refresher": "owned_database_and_disabled_embedding",
    "knowledge_scan::spawn_knowledge_scan": "private_claude_home_without_operator_projects",
    "scheduled_jobs::spawn_scheduled_jobs": "working_directory_without_repository_job_scripts",
    "settings::running_services::spawn_running_service_sync": "owned_database_and_disabled_embedding_and_rerank",
    "memory_keeper::worker::spawn_memory_keeper": "compiled_feature_disabled",
    "mail_outbox::spawn_mail_outbox": "owned_database_without_operator_mail",
    "seat_memory_worker::spawn_seat_memory_worker": "compiled_feature_disabled",
    "wright::watch::spawn_wright_worker": "compiled_feature_disabled",
    "db_cost::spawn_db_cost_monitor": "owned_database_and_compiled_db_cost_settings",
    "search_events::spawn_search_events_worker": "owned_database_without_operator_search_events",
    "analysis::rerank::spawn_rerank_warmup": "compiled_disabled_rerank_without_operator_sessions",
    "rerank_nightly::spawn_rerank_nightly": "owned_database_and_compiled_disabled_rerank",
    "judge_worker::supply_verdict::spawn_supply_verdict": "owned_database_without_operator_judged_sessions",
}


def endpoint_plan(ports):
    """Check syntax only. The isolated runner must prove live endpoint ownership."""
    require(type(ports) is dict and set(ports) == set(ENDPOINT_ROLES),
            "NORMAL_DEFAULT_ENDPOINT_ROLES")
    require(all(type(port) is int and 1024 <= port <= 65535 for port in ports.values()),
            "NORMAL_DEFAULT_ENDPOINT_PORT")
    require(len(set(ports.values())) == len(ports), "NORMAL_DEFAULT_ENDPOINT_COLLISION")
    return {role: f"127.0.0.1:{ports[role]}" for role in ENDPOINT_ROLES}


def normal_default_config(ports, database):
    """Render endpoint relocation only, retaining every other compiled default.

    The database name must have the existing disposable Cluster's syntax. That
    syntax is not evidence of ownership; the eventual caller must still use the
    owned Cluster's membership and PGDATA checks. No fallback DB is configured,
    so the default-on resync worker legitimately has no source to drain.
    Embedding is omitted: the boot loader replaces its TOML/env settings with
    compiled defaults (disabled, URL 127.0.0.1:7997). It cannot be relocated here.
    """
    endpoints = endpoint_plan(ports)
    require(isinstance(database, str)
            and re.fullmatch(r"recovery_fixture_[0-9a-f]{16}_[0-9]{1,6}", database),
            "NORMAL_DEFAULT_DATABASE_NAME")
    services = "http://" + endpoints["services"]
    values = {
        "proxy_listen_addr": endpoints["proxy"], "inspect_listen_addr": endpoints["inspect"],
        "spa_bind": endpoints["spa"],
        "spa_origins": ["http://" + endpoints["spa"], f"http://localhost:{ports['spa']}"],
        "upstream_url": "http://" + endpoints["upstream"],
        "database_url": f"postgres://postgres@{endpoints['postgres']}/{database}?sslmode=disable",
    }
    lines = [key + " = " + json.dumps(value) for key, value in values.items()]
    lines.extend(("[steerer]", "openrouter_base_url = " + json.dumps(services)))
    return ("\n".join(lines) + "\n").encode("ascii")


def validate_default_features(value):
    """Validate a supplied GET /api/settings/features result without mutating it.

    This compares source-reviewed defaults only. Candidate identity, observation
    provenance, fresh database ownership and network isolation are separate.
    """
    from journey_reliability_profiles import NORMAL_ENABLED

    require(type(value) is dict and set(value) == {"features"}
            and type(value["features"]) is list and len(value["features"]) == len(FEATURES),
            "NORMAL_DEFAULT_FEATURE_SET")
    flags = {}
    for row in value["features"]:
        key, enabled = feature_row(row)
        require(key not in flags, "NORMAL_DEFAULT_FEATURE_SET")
        flags[key] = enabled
    require(flags == {key: key in NORMAL_ENABLED for key in FEATURES},
            "NORMAL_DEFAULT_FEATURE_CHANGED")
    return flags


def default_dependency_contract():
    """Source facts and requirements only; nothing here grants runner admission."""
    return {
        "state": "NOT_RUN", "runtime_admission": "REQUIRES_VERIFIED_ISOLATED_RUNNER",
        "candidate_defaults_observed": False, "endpoint_ownership_observed": False,
        "feature_api_mutations_allowed": False,
        "endpoint_override_names": list(ENDPOINT_OVERRIDES),
        "background_worker_entrypoint": "crates/mike/src/serve/workers.rs",
        "background_worker_conditions": dict(WORKER_CONDITIONS),
        "required_private_state": ["fresh_database", "private_home_and_xdg_state",
                                   "empty_launch_queue",
                                   "owned_synthetic_project_directory", "no_real_credentials"],
        "embedding_service": {
            "compiled_enabled": False, "expected_requests": 0,
            "compiled_url": "http://127.0.0.1:7997",
            "boot_loader_resets_embedding": True, "toml_url_override_effective": False,
            "source": "crates/mike-base/src/analysis/embeddings/mod.rs",
            "loader_source": "crates/mike-base/src/config/load/mod.rs",
        },
        "workspace_registration": {
            "request_marker": "Working directory: <owned synthetic project directory>",
            "marker_placement": "Top-level system prompt or a complete <system-reminder> block; bare user-message prose is ignored.",
            "source": ["crates/mike-base/src/extract/request/project_scan.rs",
                       "crates/mike-features/src/proxy/engine/workspace_register.rs",
                       "crates/mike-base/src/machine_identity.rs"],
        },
        "implemented_worker_fixture": {
            "state": "IMPLEMENTED_NOT_RUN",
            "source": "scripts/release/journey_reliability_normal_fixture.py",
            "workspace_marker": True, "harvester_update_tag": True,
            "durable_assertions_source": "scripts/release/journey_reliability_normal_workers.py",
            "candidate_execution_observed": False,
        },
        "legacy_fixture_exercise_gaps": {
            "fixture_source": "scripts/release/journey_reliability_fixture.py",
            "workspace_capture_registration": {
                "state": "NOT_RUN", "current_request_has_project_marker": False,
                "needed": "Synthetic owned project marker plus durable workspace/session registration checks.",
            },
            "harvester": {
                "state": "NOT_RUN", "current_response_has_update_tag": False,
                "required_response_tag": '<mem:update topic="synthetic">synthetic lesson</mem:update>',
                "needed": "Synthetic update tag plus durable harvested-lesson/provenance checks.",
                "source": "crates/mike-features/src/memory_harvester/parser.rs",
            },
        },
        "limits": [
            "Feature defaults do not enumerate every background worker or prove its execution.",
            "The legacy modified-profile Cohort lacks project markers and harvester tags; the isolated normal fixture implements these triggers but requires real execution.",
            "Empty fixture state avoids operator-triggered work; it does not qualify those actions.",
            "Endpoint relocation and service fixtures do not restrict other transports or descendants.",
            "The boot loader resets embedding to disabled at http://127.0.0.1:7997; TOML cannot relocate that URL and no embedding traffic is expected.",
            "No baseline performance or normal-default candidate execution has been measured.",
        ],
    }
