use crate::state::AppState;

pub(super) fn spawn_background_workers_unless_smoke_run(state: &AppState) {
    if state.config().smoke_run {
        tracing::info!("smoke-run: skipping all background worker spawns");
        return;
    }

    crate::llm::decisions::ambient::install(state);

    crate::lesson_worker::spawn_lesson_worker(state);

    crate::resync::worker::spawn_resync_worker(state);

    crate::settings::running_services::spawn_running_service_sync(state);

    crate::embed_worker::spawn_embed_backfill(&state.knowledge_state());

    crate::knowledge_scan::spawn_knowledge_scan(&state.knowledge_state());

    crate::semantic::spawn_semantic_worker(&state.knowledge_state());

    crate::judge_worker::spawn_judge_worker(state);

    crate::dossier::worker::spawn_dossier_builder(state);

    crate::analysis::phrase_worker::spawn_phrase_filter(&state.base);

    crate::memory_keeper::worker::spawn_memory_keeper(state);

    crate::steerer::spawn_steerer_fleet(state);

    crate::quiescence_worker::spawn_quiescence_worker(state);

    crate::steerer::reply_label::demotion::spawn_steerer_demotion(state);

    crate::judge_worker::spawn_session_judge_worker(state);

    crate::judge_worker::spawn_session_end_shadow(state);

    crate::mailbox_reaper::spawn_mailbox_reaper(state);

    crate::mail_outbox::spawn_mail_outbox(state);

    crate::work_registry_worker::spawn_work_registry_worker(state);

    crate::eval_metrics::spawn_eval_metrics_worker(state);

    crate::cross_session::spawn_cross_session_worker(state);

    crate::channel_link_sweep::spawn_channel_link_sweep(state);

    crate::proxy::remote_control::spawn_repush_sweep(state);

    crate::workspace_reaper::spawn_workspace_reaper(
        state.persist().cloned(),
        state.shutdown(),
        state.config().workspace_reap_interval_secs,
        &state.tracker,
    );

    crate::launch::fleet::display::spawn_fleet_display(state);

    crate::change_record::watch::spawn_change_record_watcher(state);
    crate::change_record::code_scan::spawn_code_scanner(state);

    crate::launch::fleet::brief_version::spawn_brief_refresh(state);

    crate::launch::cli::spawn_queue_replay(state);

    crate::retention_worker::spawn_retention_sweep_worker(state);

    crate::seat_memory_worker::spawn_seat_memory_worker(state);

    crate::wright::watch::spawn_wright_worker(state);

    crate::api::handlers::spawn_health_refresher(state);

    crate::db_cost::spawn_db_cost_monitor(state);

    crate::scheduled_jobs::spawn_scheduled_jobs(state);

    crate::search_events::spawn_search_events_worker(&state.knowledge_state());

    crate::analysis::rerank::warmup::spawn_rerank_warmup(state);

    crate::rerank_nightly::spawn_rerank_nightly(&state.knowledge_state());
    crate::judge_worker::supply_verdict::spawn_supply_verdict(state);
}
