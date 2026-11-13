use super::{
    meta_assistants, meta_global, meta_launch, meta_llm_budget, meta_operations, meta_pipeline,
    meta_session_scoped, meta_workers,
};

pub static SETTINGS_META: std::sync::LazyLock<Vec<super::SettingMeta>> =
    std::sync::LazyLock::new(|| {
        let mut entries = meta_session_scoped::entries();
        entries.extend(super::meta_shell_output::entries());
        entries.extend(meta_pipeline::entries());
        entries.extend(meta_workers::entries());
        entries.extend(meta_operations::entries());
        entries.extend(meta_launch::entries());
        entries.extend(meta_global::entries());
        entries.extend(meta_llm_budget::entries());
        entries.extend(super::meta_llm_provider::entries());
        entries.extend(meta_assistants::entries());
        entries.extend(super::meta_work_registry::entries());
        entries.extend(super::meta_dossier::entries());
        entries.extend(super::meta_session_end::entries());
        entries.extend(super::meta_eval::entries());
        entries.extend(super::meta_eval_alarms::entries());
        entries.extend(super::meta_turn_judge::entries());
        entries.extend(super::meta_semantic::entries());
        entries.extend(super::meta_knowledge_embedding::entries());
        entries.extend(super::meta_turn_guard::entries());
        entries.extend(super::meta_decisions::entries());
        entries.extend(super::meta_cross_session::entries());
        entries.extend(super::meta_improvement::entries());
        entries.extend(super::meta_scheduled_jobs::entries());
        entries.extend(super::meta_memory_loop::entries());
        entries.extend(super::meta_rerank::entries());
        entries.extend(super::meta_rerank_nightly::entries());
        entries.extend(super::meta_search_ranking::entries());
        entries
    });
