use super::{
    meta_assistants, meta_global, meta_launch, meta_llm_budget, meta_operations, meta_pipeline,
    meta_workers,
};

pub static SETTINGS_META: super::registry::SettingsMeta = super::registry::SettingsMeta;

pub fn base_entries() -> Vec<super::SettingMeta> {
    let mut entries = meta_pipeline::entries();
    entries.extend(meta_workers::entries());
    entries.extend(meta_operations::entries());
    entries.extend(meta_launch::entries());
    entries.extend(meta_global::entries());
    entries.extend(meta_llm_budget::entries());
    entries.extend(super::meta_llm_provider::entries());
    entries.extend(meta_assistants::entries());
    entries.extend(super::meta_session_end::entries());
    entries.extend(super::meta_turn_judge::entries());
    entries.extend(super::meta_improvement::entries());
    entries.extend(super::meta_scheduled_jobs::entries());
    entries.extend(super::meta_rerank::entries());
    entries.extend(super::meta_rerank_nightly::entries());
    entries
}
