#[path = "common/anthropic_fixtures.rs"]
mod anthropic_fixtures;
#[path = "common/db_helpers.rs"]
mod db_helpers;
#[path = "common/seed.rs"]
mod seed;
#[path = "common/split_crates.rs"]
mod split_crates;
#[path = "common/test_db.rs"]
mod test_db;

#[path = "integration/agent_identity_unification.rs"]
mod agent_identity_unification;
#[path = "integration/agent_instances/mod.rs"]
mod agent_instances;
#[path = "integration/agents_scanner_bulk_upsert.rs"]
mod agents_scanner_bulk_upsert;
#[path = "integration/agents_scanner_db.rs"]
mod agents_scanner_db;
#[path = "integration/ask_ledger.rs"]
mod ask_ledger;
#[path = "integration/case_insensitive_names.rs"]
mod case_insensitive_names;
#[path = "integration/change_log.rs"]
mod change_log;
#[path = "integration/channel_link_sweep_db.rs"]
mod channel_link_sweep_db;
#[path = "integration/crash_recovery/mod.rs"]
mod crash_recovery;
#[path = "integration/create_steerer_no_agent/mod.rs"]
mod create_steerer_no_agent;
#[path = "integration/cross_session/mod.rs"]
mod cross_session;
#[path = "integration/db_listener_run.rs"]
mod db_listener_run;
#[path = "integration/decisions_replay_db.rs"]
mod decisions_replay_db;
#[path = "integration/dossier_entries.rs"]
mod dossier_entries;
#[path = "integration/dossier_view.rs"]
mod dossier_view;
#[path = "integration/efficiency_steerer_seeds.rs"]
mod efficiency_steerer_seeds;
#[path = "integration/embed_worker/mod.rs"]
mod embed_worker;
#[path = "integration/embeddings_http.rs"]
mod embeddings_http;
#[path = "integration/eval_bundles.rs"]
mod eval_bundles;
#[path = "integration/eval_labels.rs"]
mod eval_labels;
#[path = "integration/eval_metrics.rs"]
mod eval_metrics;
#[path = "integration/eval_runs_db.rs"]
mod eval_runs_db;
#[path = "integration/eval_runs_review.rs"]
mod eval_runs_review;
#[path = "integration/feature_toggles_service.rs"]
mod feature_toggles_service;
#[path = "integration/filter_service.rs"]
mod filter_service;
#[cfg(unix)]
#[path = "integration/fleet_criterion_paths.rs"]
mod fleet_criterion_paths;
#[path = "integration/fleet_environment.rs"]
mod fleet_environment;
#[path = "integration/fleet_run_record.rs"]
mod fleet_run_record;
#[path = "integration/fleet_timed_command.rs"]
mod fleet_timed_command;
#[path = "integration/guard_evidence.rs"]
mod guard_evidence;
#[path = "integration/guard_label_live_passes.rs"]
mod guard_label_live_passes;
#[path = "integration/heal_sentinel_session_projects.rs"]
mod heal_sentinel_session_projects;
#[path = "integration/improvement/mod.rs"]
mod improvement;
#[path = "integration/judge_quality/mod.rs"]
mod judge_quality;
#[path = "integration/lesson_proposer/mod.rs"]
mod lesson_proposer;
#[path = "integration/listen_notify_smoke.rs"]
mod listen_notify_smoke;
#[path = "integration/llm_spend_api.rs"]
mod llm_spend_api;
#[path = "integration/memory_harvester.rs"]
mod memory_harvester;
#[path = "integration/memory_recall_schema.rs"]
mod memory_recall_schema;
#[path = "integration/migration_knowledge_source_health.rs"]
mod migration_knowledge_source_health;
#[path = "integration/migration_llm_daily_spend_rollup.rs"]
mod migration_llm_daily_spend_rollup;
#[path = "integration/migration_messages_dedup.rs"]
mod migration_messages_dedup;
#[path = "integration/migration_stop_harness_turns.rs"]
mod migration_stop_harness_turns;
#[path = "integration/migrations_smoke.rs"]
mod migrations_smoke;
#[path = "integration/never_fired_db.rs"]
mod never_fired_db;
#[path = "integration/origin_backfill.rs"]
mod origin_backfill;
#[path = "integration/predecessor_request_id.rs"]
mod predecessor_request_id;
#[path = "integration/project_knowledge_http.rs"]
mod project_knowledge_http;
#[path = "integration/prompt_classification.rs"]
mod prompt_classification;
#[path = "integration/remote_control_push_wake.rs"]
mod remote_control_push_wake;
#[path = "integration/remote_control_roster.rs"]
mod remote_control_roster;
#[path = "integration/request_deltas_api.rs"]
mod request_deltas_api;
#[path = "integration/request_responses.rs"]
mod request_responses;
#[path = "integration/requests_check_constraints.rs"]
mod requests_check_constraints;
#[path = "integration/rerank_settings.rs"]
mod rerank_settings;
#[path = "integration/resumed_session_channel_link.rs"]
mod resumed_session_channel_link;
#[path = "integration/resync_engine.rs"]
mod resync_engine;
#[path = "integration/resync_worker.rs"]
mod resync_worker;
#[path = "integration/retention_worker_db.rs"]
mod retention_worker_db;
#[path = "integration/row_changed_filters.rs"]
mod row_changed_filters;
#[path = "integration/runtime_codex/mod.rs"]
mod runtime_codex;
#[path = "integration/seat_memory/mod.rs"]
mod seat_memory;
#[path = "integration/semantic/mod.rs"]
mod semantic;
#[path = "integration/session_delivery_api/mod.rs"]
mod session_delivery_api;
#[path = "integration/session_embeddings.rs"]
mod session_embeddings;
#[path = "integration/session_end/mod.rs"]
mod session_end;
#[path = "integration/session_flow_api.rs"]
mod session_flow_api;
#[path = "integration/session_flow_listener_recovery/mod.rs"]
mod session_flow_listener_recovery;
#[path = "integration/session_flow_notifications/mod.rs"]
mod session_flow_notifications;
#[path = "integration/session_mailbox.rs"]
mod session_mailbox;
#[path = "integration/session_narrative_overview.rs"]
mod session_narrative_overview;
#[path = "integration/session_project_assignment.rs"]
mod session_project_assignment;
#[path = "integration/settings_service.rs"]
mod settings_service;
#[path = "integration/steer_guard/mod.rs"]
mod steer_guard;
#[path = "integration/steer_promotion/mod.rs"]
mod steer_promotion;
#[path = "integration/steering_facts.rs"]
mod steering_facts;
#[path = "integration/store/mod.rs"]
mod store;
#[path = "integration/store_operator_turns.rs"]
mod store_operator_turns;
#[path = "integration/structural_reachability_scope.rs"]
mod structural_reachability_scope;
#[path = "integration/tool_uses_view.rs"]
mod tool_uses_view;
#[path = "integration/turn_facts.rs"]
mod turn_facts;
#[path = "integration/turn_verdicts_db.rs"]
mod turn_verdicts_db;
#[path = "integration/verifier_input_budget.rs"]
mod verifier_input_budget;
#[path = "integration/wiring_findings_db.rs"]
mod wiring_findings_db;
#[path = "integration/work_threads_http.rs"]
mod work_threads_http;
#[path = "integration/workspace_reaper_db.rs"]
mod workspace_reaper_db;
#[path = "integration/workspace_registration.rs"]
mod workspace_registration;

#[cfg(all(feature = "test-seams", any(target_os = "linux", target_os = "macos")))]
#[path = "integration/managed_execution.rs"]
mod managed_execution;

#[cfg(all(feature = "test-seams", any(target_os = "linux", target_os = "macos")))]
#[path = "integration/fleet_managed_control.rs"]
mod fleet_managed_control;

#[path = "integration/fleet_admission.rs"]
mod fleet_admission;

#[path = "integration/fleet_shutdown/mod.rs"]
mod fleet_shutdown;

#[cfg(all(feature = "test-seams", unix))]
#[path = "integration/fleet_build_admission.rs"]
mod fleet_build_admission;

#[cfg(all(feature = "test-seams", any(target_os = "linux", target_os = "macos")))]
#[path = "integration/managed_admission_db.rs"]
mod managed_admission_db;

#[cfg(feature = "test-seams")]
#[path = "integration/fleet_late_gate/mod.rs"]
mod fleet_late_gate;

#[path = "integration/connection_info.rs"]
mod connection_info;
#[path = "integration/local_access_http.rs"]
mod local_access_http;
#[path = "integration/local_access_policy.rs"]
mod local_access_policy;
#[path = "integration/migration_checksums.rs"]
mod migration_checksums;

#[path = "integration/installed_database_provider.rs"]
mod installed_database_provider;
