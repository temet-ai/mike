#[path = "common/anthropic_fixtures.rs"]
mod anthropic_fixtures;
#[path = "common/mod.rs"]
mod common;
#[path = "common/db_cap_helpers.rs"]
mod db_cap_helpers;
#[path = "common/db_helpers.rs"]
mod db_helpers;
#[path = "common/filter_proxy.rs"]
mod filter_proxy;
#[path = "common/seed.rs"]
mod seed;
#[path = "common/sse.rs"]
mod sse;
#[path = "common/test_db.rs"]
mod test_db;
#[path = "common/upstream.rs"]
mod upstream;

#[path = "e2e/agent_lesson_injection.rs"]
mod agent_lesson_injection;
#[path = "e2e/agents_api.rs"]
mod agents_api;
#[path = "e2e/anomalies_api.rs"]
mod anomalies_api;
#[path = "e2e/anthropic_integration_persistence.rs"]
mod anthropic_integration_persistence;
#[path = "e2e/api_db_fallback.rs"]
mod api_db_fallback;
#[path = "e2e/api_file_writes.rs"]
mod api_file_writes;
#[path = "e2e/api_message_receipt.rs"]
mod api_message_receipt;
#[path = "e2e/api_session_etag.rs"]
mod api_session_etag;
#[path = "e2e/api_session_request_deltas.rs"]
mod api_session_request_deltas;
#[path = "e2e/api_session_summary_parity.rs"]
mod api_session_summary_parity;
#[path = "e2e/api_tool_uses.rs"]
mod api_tool_uses;
#[path = "e2e/api_turn_tool_calls.rs"]
mod api_turn_tool_calls;
#[path = "e2e/compose_prompt.rs"]
mod compose_prompt;
#[path = "e2e/composition_api.rs"]
mod composition_api;
#[path = "e2e/db_listener_smoke.rs"]
mod db_listener_smoke;
#[path = "e2e/diagnostics_api.rs"]
mod diagnostics_api;
#[path = "e2e/dossier_builder.rs"]
mod dossier_builder;
#[path = "e2e/eligibility_excludes_noise.rs"]
mod eligibility_excludes_noise;
#[path = "e2e/embed_drain.rs"]
mod embed_drain;
#[path = "e2e/embedding_graph_api.rs"]
mod embedding_graph_api;
#[path = "e2e/embeddings_recall.rs"]
mod embeddings_recall;
#[path = "e2e/file_read_compression.rs"]
mod file_read_compression;
#[path = "e2e/filter_rule_firing_counts_api.rs"]
mod filter_rule_firing_counts_api;
#[path = "e2e/filter_rules_api.rs"]
mod filter_rules_api;
#[path = "e2e/filter_rules_style_check.rs"]
mod filter_rules_style_check;
#[path = "e2e/find_text_endpoint.rs"]
mod find_text_endpoint;
#[path = "e2e/find_text_session_pagination.rs"]
mod find_text_session_pagination;
#[path = "e2e/in_flight_lifecycle.rs"]
mod in_flight_lifecycle;
#[path = "e2e/judge_labeling.rs"]
mod judge_labeling;
#[path = "e2e/judge_promotion.rs"]
mod judge_promotion;
#[path = "e2e/lesson_injection.rs"]
mod lesson_injection;
#[path = "e2e/lesson_relations_api.rs"]
mod lesson_relations_api;
#[path = "e2e/mailbox_busy_recipient_delivery.rs"]
mod mailbox_busy_recipient_delivery;
#[path = "e2e/mailbox_lane_gate.rs"]
mod mailbox_lane_gate;
#[path = "e2e/mailbox_message_context.rs"]
mod mailbox_message_context;
#[path = "e2e/mailbox_open_threads.rs"]
mod mailbox_open_threads;
#[path = "e2e/mailbox_provenance.rs"]
mod mailbox_provenance;
#[path = "e2e/mailbox_receipt.rs"]
mod mailbox_receipt;
#[path = "e2e/mailbox_staleness_and_failed_sends.rs"]
mod mailbox_staleness_and_failed_sends;
#[path = "e2e/mailbox_strand_prevention.rs"]
mod mailbox_strand_prevention;
#[path = "e2e/mailbox_tags_addressing.rs"]
mod mailbox_tags_addressing;
#[path = "e2e/mailbox_threads.rs"]
mod mailbox_threads;
#[path = "e2e/memory_consolidation_audit_api.rs"]
mod memory_consolidation_audit_api;
#[path = "e2e/narrative_overview_api.rs"]
mod narrative_overview_api;
#[path = "e2e/operator_turn_lifecycle.rs"]
mod operator_turn_lifecycle;
#[path = "e2e/operator_turn_model_28.rs"]
mod operator_turn_model_28;
#[path = "e2e/operator_turns_api.rs"]
mod operator_turns_api;
#[path = "e2e/persistence_restart_smoke.rs"]
mod persistence_restart_smoke;
#[path = "e2e/persistence_session_summary.rs"]
mod persistence_session_summary;
#[path = "e2e/phase18_new_endpoints.rs"]
mod phase18_new_endpoints;
#[path = "e2e/prompt_kind_18_1b.rs"]
mod prompt_kind_18_1b;
#[path = "e2e/proxy_declared_tool_loop.rs"]
mod proxy_declared_tool_loop;
#[path = "e2e/recall_api.rs"]
mod recall_api;
#[path = "e2e/recall_injection.rs"]
mod recall_injection;
#[path = "e2e/request_steering_api.rs"]
mod request_steering_api;
#[path = "e2e/request_tools_roundtrip.rs"]
mod request_tools_roundtrip;
#[path = "e2e/requests_list_db.rs"]
mod requests_list_db;
#[path = "e2e/response_split_e2e.rs"]
mod response_split_e2e;
#[path = "e2e/restart_regression.rs"]
mod restart_regression;
#[path = "e2e/row_changed_per_request.rs"]
mod row_changed_per_request;
#[path = "e2e/semantic_api/mod.rs"]
mod semantic_api;
#[path = "e2e/session_filter_rules_api.rs"]
mod session_filter_rules_api;
#[path = "e2e/session_fleet_api.rs"]
mod session_fleet_api;
#[path = "e2e/session_head_prompt_kind_counts_136.rs"]
mod session_head_prompt_kind_counts_136;
#[path = "e2e/session_judge_related_sessions.rs"]
mod session_judge_related_sessions;
#[path = "e2e/session_overview.rs"]
mod session_overview;
#[path = "e2e/session_vector_sweep.rs"]
mod session_vector_sweep;
#[path = "e2e/sessions_list_db.rs"]
mod sessions_list_db;
#[path = "e2e/steerer_enrolment_count_api.rs"]
mod steerer_enrolment_count_api;
#[path = "e2e/steerer_openrouter_key_persistence.rs"]
mod steerer_openrouter_key_persistence;
#[path = "e2e/steering_api.rs"]
mod steering_api;
#[path = "e2e/stream_block_gap_recovery.rs"]
mod stream_block_gap_recovery;
#[path = "e2e/stream_chunks_db.rs"]
mod stream_chunks_db;
#[path = "e2e/tool_result_post_restart.rs"]
mod tool_result_post_restart;
#[path = "e2e/tool_results_api.rs"]
mod tool_results_api;
#[path = "e2e/turn_disposition.rs"]
mod turn_disposition;
#[path = "e2e/verifier_input_budget.rs"]
mod verifier_input_budget;
#[path = "e2e/volatile_lines_api.rs"]
mod volatile_lines_api;
#[path = "e2e/weasel_intercept_evidence.rs"]
mod weasel_intercept_evidence;
#[path = "e2e/write_path_messages_bridge.rs"]
mod write_path_messages_bridge;
#[path = "e2e/write_path_tool_results.rs"]
mod write_path_tool_results;
#[path = "e2e/write_path_tool_uses.rs"]
mod write_path_tool_uses;
