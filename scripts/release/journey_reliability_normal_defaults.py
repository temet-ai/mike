"""Complete advertised settings and compiled boot-default observations.

An empty selected file supplies the same candidate's compiled boot baseline.
Every serialized field is compared, except explicit owned endpoint relocations
and installation-derived data/log paths. Runtime metadata is checked in full,
with the complete source-reviewed key inventory and no database setting rows.
This is observation of a trusted candidate, not independent proof of its code.
"""
import hashlib
import json
import tomllib

from journey_process import require, captured_run, installation_identity, runtime_environment
from journey_reliability import api_json, owned_database
from journey_reliability_defaults import ENDPOINT_OVERRIDES, validate_default_features
from journey_reliability_isolation import FEATURES, verify_service_prerequisites
from journey_reliability_profiles import NORMAL_ENABLED

SETTINGS_KEYS = frozenset("""
launch_use_sidecar
analysis.categories.avoiding_work.enabled
analysis.categories.avoiding_work.false_positive_filters
analysis.categories.deflecting_blame.enabled
analysis.categories.deflecting_blame.false_positive_filters
analysis.categories.faking_understanding.enabled
analysis.categories.faking_understanding.false_positive_filters
analysis.categories.justifying_shortcuts.enabled
analysis.categories.justifying_shortcuts.false_positive_filters
analysis.categories.overselling.enabled
analysis.categories.overselling.false_positive_filters
analysis.enabled
assistant.anthropic_auth_mode
assistant.provider
cargo_sweep.enabled
composition_enabled
compressor_loop.enabled
compression_dedup.enabled
directives.proposer.min_confirmed
directives.proposer.min_sessions
directives.proposer.sources
directives.proposer.window_days
dossier.backstop_interval_secs
dossier.backstop_lookback_hours
dossier.batch_sessions
dossier.entries.enabled
dossier.flush_interval_ms
dossier.session_settle_minutes
embedding.chunk_max_chars
embedding.enabled
embedding.model
embedding.url
eval.alarm.budget_refusals.min_refused
eval.alarm.budget_refusals.min_share
eval.alarm.change_without_gate.grace_hours
eval.alarm.judge_call_loop.max_calls_per_row
eval.alarm.judge_call_loop.min_calls
eval.alarm.judge_stalled.hours
eval.alarm.judge_stalled.min_waiting
eval.alarm.label_band.corrected_max
eval.alarm.label_band.corrected_min
eval.alarm.label_band.enabled
eval.alarm.label_mix.min_chi2
eval.alarm.label_mix.min_turns
eval.alarm.label_mix.min_tvd
eval.alarm.metric_drop.enabled
eval.alarm.metric_drop.min_n
eval.alarm.shadow_disagreement.enabled
eval.alarm.shadow_disagreement.min_kappa
eval.alarm.shadow_disagreement.min_n
eval.alarm.unjudged_backlog.min_turns
eval.labels.holdout_percent
eval.relay_labels_enabled
eval.rollup.enabled
eval.rollup.hour_utc
file_read_compression.enabled_producers
file_read_compression.forward_share
file_read_compression.grace_turns
file_read_compression.head_lines
file_read_compression.min_bytes
file_read_compression.mode
file_read_compression.protected_paths
file_read_compression.tail_lines
guard.calibration.input_mode
guard.calibration.min_labelled
guard.calibration.window_days
guard.threshold.allow_lowering_for_wake
guard.threshold.ceiling
guard.threshold.cooldown_days
guard.threshold.hysteresis
guard.threshold.min_labelled
guard.threshold.mode
guard.threshold.steerer_bar
guard.threshold.weight.next_message
guard.threshold.weight.operator
guard.threshold.weight.session_end
guard.threshold.weight.session_reply
health.llm_failures_degrade_status
inspector_cache.api_response_budget_bytes
inspector_cache.body_entry_cap
inspector_cache.detail_entry_cap
inspector_cache.enabled
inspector_cache.idb_budget_bytes
inspector_cache.memory_budget_bytes
judge_lessons.auto_promote_min_evidence
judge_lessons.maintenance_max_per_day
judge_lessons.max_per_day
judge_lessons.min_confidence
judge_lessons.model
judge_lessons.quiet_end_hour_utc
judge_lessons.quiet_idle_secs
judge_lessons.quiet_max_per_day
judge_lessons.quiet_start_hour_utc
judge_worker.batch_size
judge_worker.confidence_threshold
judge_worker.interval_secs
judge_worker.max_output_tokens
judge_worker.model
judge_worker.reasoning_effort
judge_worker.system_prompt
launch_auto_trust
launch_backend
launch_brief_inline_max
launch_ca_cert
launch_channel_check_enabled
launch_channel_check_interval_secs
launch_channel_check_timeout_secs
launch_claude_args
launch_kill_wait_secs
launch_max_surfaces
launch_no_proxy
launch_proxy_forward_url
launch_warn_surfaces
launch_worktree_config
learning_audit.enabled
learning_promote.enabled
lesson_relations.adjudicator_model
lesson_relations.candidate_distance_threshold
lesson_relations.enabled
lesson_relations.proposer_model
lesson_worker.dedup_threshold
lesson_worker.gc_interval_secs
lesson_worker.max_lessons_per_project
lesson_worker.nightly_consolidation_hour
lesson_worker.session_consolidation_idle_secs
lessons.skills.max_per_project
lessons_commit.enabled
llm.budget.eval.share_pct
llm.budget.judge_worker.share_pct
llm.budget.lesson_proposer.share_pct
llm.budget.lesson_relations.share_pct
llm.budget.other.share_pct
llm.budget.session_judge.share_pct
llm.budget.steerer.share_pct
llm.ceiling_max_calls
llm.ceiling_window_secs
llm.daily_spend_ceiling_usd
llm.max_output_tokens
llm.rate_limit_per_minute
llm.reasoning_effort
llm.record_usage
llm.spend_alert_fraction
log_format
log_level
log_max_files
mailbox_delivery_ttl_minutes
mailbox_max_deliveries_per_turn
mailbox_reject_send_to_ended_session
mailbox_stale_minutes
max_in_flight
max_request_body_bytes
max_response_body_bytes
max_store_bytes
prompt_assistant.capture
prompt_assistant.codex_model
prompt_assistant.daily_budget_usd
prompt_assistant.effort
prompt_assistant.instructions
prompt_assistant.keep_days
prompt_assistant.max_turns
prompt_assistant.may_apply
prompt_assistant.model
prompt_assistant.read_edits
prompt_assistant.read_memory
prompt_assistant.read_other_projects
prompt_assistant.read_project_instructions
prompt_assistant.read_prompt
prompt_assistant.read_versions
recall_injector.max_lessons_per_topic
reminders.inject_lessons
reminders.max_lessons_to_inject
semantic.allow_remote_embedding
semantic.batch_turns
semantic.cluster_floor_quantile
semantic.cluster_min_members
semantic.cluster_min_sessions
semantic.drift_min_current
semantic.drift_z
semantic.ending_chars
semantic.floor_quantile
semantic.floor_sample_turns
semantic.inline_timeout_ms
semantic.relapse_evidence
semantic.retrieval_k
semantic.retrieval_scope
semantic.sweep_interval_secs
semantic.tagger_min_score
session_end_shadow.batch_size
session_end_shadow.bundle_id
session_end_shadow.interval_secs
session_end_shadow.lookback_hours
session_end_shadow.sample_rate
session_judge.batch_size
session_judge.live_bundle_id
session_judge.max_attempts_per_day
session_judge.max_chars_per_turn
session_judge.max_output_tokens
session_judge.max_turns_in_prompt
session_judge.min_session_age_secs
session_judge.model
session_judge.poll_interval_secs
session_judge.reasoning_effort
session_judge.rejudge_daily_call_cap
session_judge.request_timeout_secs
session_judge.system_prompt
session_judge.v3_lookback_hours
steerer.intake_criteria.input_budget
steerer.max_output_tokens
steerer.model
steerer.openrouter_api_key_env
steerer.quiescence_minutes
steerer.quiet_period_secs
steerer.reasoning_effort
steerer.refocus.confidence_threshold
steerer.refocus.debounce_turns
steerer.refocus.enabled
steerer.request_timeout_secs
steerer.retain_pass_verdicts
steerer.session_note_limit
steerer.session_note_window_minutes
steerer.verifier.input_budget
steerer.wrong_note_mute_hours
turn_judge.immediate
turn_judge.immediate_delay_ms
turn_judge.live_bundle_id
turn_judge.live_window_days
turn_judge.rejudge_daily_call_cap
usage_audit.enabled
weasel_intercept.correction_prompt
weasel_intercept.forbidden_words
work_registry.wake_mode
workspace_reap_interval_secs
""".split())
OVERLAYS_SQL = """SELECT json_build_object('settings', (SELECT count(*) FROM config.settings),
 'toggles', (SELECT coalesce(json_agg(t ORDER BY key),'[]'::json) FROM
 (SELECT key,enabled,operator_set FROM config.feature_toggles) t))::text"""


def stable_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def flatten(value, prefix=""):
    result = {}
    for key, item in value.items():
        name = prefix + key
        if isinstance(item, dict):
            result.update(flatten(item, name + "."))
        else:
            result[name] = item
    return result


def boot_snapshot(binary, config, home, cwd):
    raw = captured_run([str(binary), "--config", str(config), "print-config"],
                       cwd, runtime_environment(home), timeout=20)
    require(b"# BOOT CONFIG:" in raw and b"# LIVE OVERLAY:" in raw, "NORMAL_DEFAULT_BOOT_REPORT")
    return flatten(tomllib.loads(raw.split(b"# LIVE OVERLAY:", 1)[0].decode("utf-8")))


def validate_boot_defaults(actual, defaults, config, baseline_config, endpoints, database):
    require(isinstance(actual, dict) and isinstance(defaults, dict), "NORMAL_DEFAULT_BOOT_SHAPE")
    expected = dict(defaults)
    relocations = {
        "proxy_listen_addr": f"127.0.0.1:{endpoints['proxy']}",
        "inspect_listen_addr": f"127.0.0.1:{endpoints['inspect']}",
        "spa_bind": f"127.0.0.1:{endpoints['spa']}",
        "spa_origins": [f"http://127.0.0.1:{endpoints['spa']}", f"http://localhost:{endpoints['spa']}"],
        "upstream_url": f"http://127.0.0.1:{endpoints['upstream']}",
        "database_url": f"postgres://postgres@127.0.0.1:{endpoints['postgres']}/{database}",
        "finite_url": f"http://127.0.0.1:{endpoints['services']}",
        "steerer.openrouter_base_url": f"http://127.0.0.1:{endpoints['services']}",
    }
    require(set(relocations) == set(ENDPOINT_OVERRIDES), "NORMAL_DEFAULT_BOOT_RELOCATION_SET")
    for selected, snapshot in ((config, actual), (baseline_config, defaults)):
        identity, record = installation_identity(selected)
        private_root = record.parent.parent
        require(snapshot.get("data_dir") == str(private_root / "data")
                and snapshot.get("log_dir") == str(private_root / "logs"), "NORMAL_DEFAULT_BOOT_PRIVATE_PATHS")
    expected.update(relocations, data_dir=actual["data_dir"], log_dir=actual["log_dir"])
    require(actual == expected, "NORMAL_DEFAULT_BOOT_CHANGED")
    require(actual.get("smoke_run") is False and actual.get("persist_enabled") is True
            and actual.get("db_resync_enabled") is True and actual.get("db_fallback_enabled") is True
            and actual.get("embedding.enabled") is False
            and actual.get("embedding.url") == "http://127.0.0.1:7997"
            and "local_database_url" not in actual, "NORMAL_DEFAULT_BOOT_PREREQUISITES")
    return {"serialized_boot_fields": len(actual), "compiled_boot_defaults_preserved": True,
            "boot_snapshot_sha256": stable_hash(actual), "endpoint_relocations": len(relocations)}


def validate_settings(value):
    require(isinstance(value, dict) and set(value) == {"settings", "steerer_openrouter_configured_via_env"}
            and value["steerer_openrouter_configured_via_env"] is False
            and isinstance(value["settings"], list), "NORMAL_DEFAULT_SETTINGS_SHAPE")
    settings = {}
    for row in value["settings"]:
        require(isinstance(row, dict) and row.get("key") in SETTINGS_KEYS
                and row["key"] not in settings and "default" in row and "value" in row,
                "NORMAL_DEFAULT_SETTINGS_KEYS")
        # Canonical JSON comparison distinguishes true from 1, unlike Python ==.
        require(stable_hash(row["value"]) == stable_hash(row["default"]), "NORMAL_DEFAULT_SETTING_CHANGED")
        settings[row["key"]] = row["value"]
    require(set(settings) == SETTINGS_KEYS, "NORMAL_DEFAULT_SETTINGS_KEYS")
    require(settings["embedding.enabled"] is False
            and settings["embedding.url"] == "http://127.0.0.1:7997"
            and settings["launch_backend"] == "none", "NORMAL_DEFAULT_SETTINGS_PREREQUISITES")
    return settings


def observe_defaults(port, health, cluster, database, budget, report):
    verify_service_prerequisites(health)
    flags = validate_default_features(api_json(port, "/api/settings/features", budget))
    settings = validate_settings(api_json(port, "/api/settings", budget))
    owned_database(cluster, database)
    overlays = json.loads(cluster.sql(database, "BEGIN READ ONLY; SET LOCAL statement_timeout=5000; "
                                      + OVERLAYS_SQL + "; COMMIT;"))
    require(isinstance(overlays, dict) and set(overlays) == {"settings", "toggles"}
            and type(overlays["settings"]) is int and overlays["settings"] == 0
            and isinstance(overlays["toggles"], list),
            "NORMAL_DEFAULT_DATABASE_OVERRIDE")
    keys = set()
    for row in overlays["toggles"]:
        require(isinstance(row, dict) and set(row) == {"key", "enabled", "operator_set"}
                and row["key"] in FEATURES and row["key"] not in keys
                and type(row["enabled"]) is bool and type(row["operator_set"]) is bool,
                "NORMAL_DEFAULT_DATABASE_TOGGLE")
        require(not row["operator_set"] or row["enabled"] == (row["key"] in NORMAL_ENABLED),
                "NORMAL_DEFAULT_DATABASE_OVERRIDE")
        keys.add(row["key"])
    # Historical migrations flag some factory-enabled rows operator_set=true.
    # Preserve those exact factory rows rather than rewriting any feature API.
    overlay_digest = stable_hash(overlays)
    require(report.get("database_defaults_sha256", overlay_digest) == overlay_digest,
            "NORMAL_DEFAULT_DATABASE_OVERRIDE_CHANGED")
    report["database_defaults_sha256"] = overlay_digest
    digest = stable_hash(settings)
    require(report.get("settings_snapshot_sha256", digest) == digest, "NORMAL_DEFAULT_SETTINGS_CHANGED")
    report.update(effective_feature_flags=flags, settings_snapshot_sha256=digest,
                  effective_settings_count=len(settings), candidate_effective_defaults_verified=True)
    report["feature_observations"] += 1
    report["settings_observations"] = report.get("settings_observations", 0) + 1
