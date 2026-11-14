use crate::settings::SettingMeta;

pub fn entries() -> Vec<SettingMeta> {
    let mut entries = super::meta_session_scoped::entries();
    entries.extend(super::meta_shell_output::entries());
    entries.extend(super::meta_dossier::entries());
    entries.extend(super::meta_eval::entries());
    entries.extend(super::meta_eval_alarms::entries());
    entries.extend(super::meta_turn_guard::entries());
    entries.extend(super::meta_cross_session::entries());
    entries
}
