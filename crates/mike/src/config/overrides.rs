use super::overrides_analysis::apply_analysis_override;
use super::overrides_launch::apply_launch_override;
use super::schema::ProxyConfig;

pub(crate) fn apply_settings_overrides(
    cfg: &mut ProxyConfig,
    rows: &[(String, serde_json::Value)],
) {
    for (key, value) in rows {
        let key = key.as_str();
        let _handled = apply_composition_and_embedding_override(cfg, key, value)
            || apply_analysis_override(cfg, key, value)
            || apply_steerer_override(cfg, key, value)
            || apply_turn_judge_override(cfg, key, value)
            || apply_session_judge_override(cfg, key, value)
            || apply_lesson_worker_override(cfg, key, value)
            || apply_lesson_relations_override(cfg, key, value)
            || apply_resource_cap_override(cfg, key, value)
            || apply_mailbox_override(cfg, key, value)
            || apply_logging_override(cfg, key, value)
            || apply_launch_override(cfg, key, value);
    }
}

fn apply_composition_and_embedding_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "composition_enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.composition_enabled = v;
            }
        }

        "embedding.enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.embedding.enabled = v;
            }
        }
        "embedding.url" => {
            if let Some(v) = value.as_str() {
                cfg.embedding.url = v.to_owned();
            }
        }
        "embedding.model" => {
            if let Some(v) = value.as_str() {
                cfg.embedding.model = v.to_owned();
            }
        }
        "embedding.chunk_max_chars" => {
            if let Some(v) = value.as_u64() {
                cfg.embedding.chunk_max_chars = v as usize;
            }
        }
        _ => return false,
    }
    true
}

fn apply_steerer_override(cfg: &mut ProxyConfig, key: &str, value: &serde_json::Value) -> bool {
    match key {
        "steerer.model" => {
            if let Some(v) = value.as_str() {
                cfg.steerer.model = v.to_owned();
            }
        }
        "steerer.request_timeout_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.steerer.request_timeout_secs = v;
            }
        }
        "steerer.quiet_period_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.steerer.quiet_period_secs = v;
            }
        }
        "steerer.quiescence_minutes" => {
            if let Some(v) = value.as_u64() {
                cfg.steerer.quiescence_minutes = v;
            }
        }
        "steerer.refocus.enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.steerer.refocus.enabled = v;
            }
        }
        "steerer.refocus.confidence_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.steerer.refocus.confidence_threshold = v as f32;
            }
        }
        "steerer.refocus.debounce_turns" => {
            if let Some(v) = value.as_u64() {
                cfg.steerer.refocus.debounce_turns = v as u32;
            }
        }
        _ => return false,
    }
    true
}

fn apply_turn_judge_override(cfg: &mut ProxyConfig, key: &str, value: &serde_json::Value) -> bool {
    match key {
        "judge_worker.model" => {
            if let Some(v) = value.as_str() {
                cfg.steerer.judge.model = if v.is_empty() {
                    None
                } else {
                    Some(v.to_owned())
                };
            }
        }
        "judge_worker.system_prompt" => {
            if let Some(v) = value.as_str() {
                cfg.steerer.judge.system_prompt = v.to_owned();
            }
        }
        "judge_worker.batch_size" => {
            if let Some(v) = value.as_i64() {
                cfg.steerer.judge.batch_size = v;
            }
        }
        "judge_worker.confidence_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.steerer.judge.confidence_threshold = v as f32;
            }
        }
        "judge_worker.interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.steerer.judge.interval_secs = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_session_judge_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "session_judge.model" => {
            if let Some(v) = value.as_str()
                && !v.is_empty()
            {
                cfg.session_judge.model = v.to_owned();
            }
        }
        "session_judge.system_prompt" => {
            if let Some(v) = value.as_str() {
                cfg.session_judge.system_prompt = v.to_owned();
            }
        }
        "session_judge.batch_size" => {
            if let Some(v) = value.as_i64() {
                cfg.session_judge.batch_size = v;
            }
        }
        "session_judge.min_session_age_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.session_judge.min_session_age_secs = v;
            }
        }
        "session_judge.max_turns_in_prompt" => {
            if let Some(v) = value.as_u64() {
                cfg.session_judge.max_turns_in_prompt = v as usize;
            }
        }
        "session_judge.poll_interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.session_judge.poll_interval_secs = v;
            }
        }
        "session_judge.request_timeout_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.session_judge.request_timeout_secs = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_lesson_worker_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "lesson_worker.dedup_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.lesson_worker.dedup_threshold = v as f32;
            }
        }
        "lesson_worker.max_lessons_per_project" => {
            if let Some(v) = value.as_i64() {
                cfg.lesson_worker.max_lessons_per_project = v;
            }
        }
        "lesson_worker.session_consolidation_idle_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.lesson_worker.session_consolidation_idle_secs = v;
            }
        }
        "lesson_worker.nightly_consolidation_hour" => {
            if let Some(v) = value.as_u64() {
                cfg.lesson_worker.nightly_consolidation_hour = v as u8;
            }
        }
        "lesson_worker.gc_interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.lesson_worker.gc_interval_secs = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_lesson_relations_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "lesson_relations.enabled" => {
            if let Some(v) = value.as_bool() {
                cfg.lesson_relations.enabled = v;
            }
        }
        "lesson_relations.proposer_model" => {
            if let Some(v) = value.as_str() {
                cfg.lesson_relations.proposer_model = v.to_owned();
            }
        }
        "lesson_relations.adjudicator_model" => {
            if let Some(v) = value.as_str() {
                cfg.lesson_relations.adjudicator_model = v.to_owned();
            }
        }
        "lesson_relations.candidate_distance_threshold" => {
            if let Some(v) = value.as_f64() {
                cfg.lesson_relations.candidate_distance_threshold = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_resource_cap_override(
    cfg: &mut ProxyConfig,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "max_request_body_bytes" => {
            if let Some(v) = value.as_u64() {
                cfg.max_request_body_bytes = v;
            }
        }
        "max_response_body_bytes" => {
            if let Some(v) = value.as_u64() {
                cfg.max_response_body_bytes = v;
            }
        }
        "max_in_flight" => {
            if let Some(v) = value.as_u64() {
                cfg.max_in_flight = v;
            }
        }
        "max_store_bytes" => {
            if let Some(v) = value.as_u64() {
                cfg.max_store_bytes = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_mailbox_override(cfg: &mut ProxyConfig, key: &str, value: &serde_json::Value) -> bool {
    match key {
        "mailbox_stale_minutes" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_stale_minutes = v;
            }
        }
        "mailbox_redelivery_max_attempts" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_redelivery_max_attempts = v as i32;
            }
        }
        "mailbox_redelivery_backoff_base_secs" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_redelivery_backoff_base_secs = v;
            }
        }
        "mailbox_redelivery_backoff_cap_secs" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_redelivery_backoff_cap_secs = v;
            }
        }
        "mailbox_delivery_ttl_minutes" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_delivery_ttl_minutes = v;
            }
        }
        "mailbox_max_deliveries_per_turn" => {
            if let Some(v) = value.as_i64() {
                cfg.mailbox_max_deliveries_per_turn = v;
            }
        }
        "mailbox_reject_send_to_ended_session" => {
            if let Some(v) = value.as_bool() {
                cfg.mailbox_reject_send_to_ended_session = v;
            }
        }
        "workspace_reap_interval_secs" => {
            if let Some(v) = value.as_u64() {
                cfg.workspace_reap_interval_secs = v;
            }
        }
        _ => return false,
    }
    true
}

fn apply_logging_override(cfg: &mut ProxyConfig, key: &str, value: &serde_json::Value) -> bool {
    match key {
        "log_level" => {
            if let Some(v) = value.as_str() {
                cfg.log_level = v.to_owned();
            }
        }
        "log_format" => {
            if let Some(v) = value.as_str() {
                cfg.log_format = v.to_owned();
            }
        }
        "log_max_files" => {
            if let Some(v) = value.as_u64() {
                cfg.log_max_files = v as usize;
            }
        }

        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::load::CliOverrides;
    use clap::Parser;

    fn parse(args: &[&str]) -> anyhow::Result<ProxyConfig> {
        let overrides = CliOverrides::try_parse_from(args)?;
        crate::config::load::load(overrides)
    }

    #[test]
    fn apply_settings_overrides_folds_int_bool_enum_keys() -> anyhow::Result<()> {
        let mut cfg = parse(&["mike", "--persist-enabled", "0"])?;

        let default_log_max_files = cfg.log_max_files;
        let default_composition = cfg.composition_enabled;
        let default_log_level = cfg.log_level.clone();

        let rows: Vec<(String, serde_json::Value)> = vec![
            (
                "log_max_files".to_owned(),
                serde_json::Value::Number((default_log_max_files as u64 + 10).into()),
            ),
            (
                "composition_enabled".to_owned(),
                serde_json::Value::Bool(!default_composition),
            ),
            ("log_level".to_owned(), serde_json::json!("debug")),
            (
                "reminders.inject_lessons".to_owned(),
                serde_json::Value::Bool(false),
            ),
        ];

        super::apply_settings_overrides(&mut cfg, &rows);

        assert_eq!(
            cfg.log_max_files,
            default_log_max_files + 10,
            "Int key must be folded onto cfg.log_max_files"
        );
        assert_eq!(
            cfg.composition_enabled, !default_composition,
            "Bool key must be folded onto cfg.composition_enabled"
        );
        assert_eq!(
            cfg.log_level, "debug",
            "Enum-as-String key must be folded onto cfg.log_level"
        );
        assert_ne!(
            cfg.log_level, default_log_level,
            "log_level must have changed from the default"
        );

        Ok(())
    }

    #[test]
    fn mailbox_strand_prevention_settings_default_and_fold() -> anyhow::Result<()> {
        let mut cfg = parse(&["mike", "--persist-enabled", "0"])?;

        assert_eq!(cfg.mailbox_delivery_ttl_minutes, 60);
        assert_eq!(cfg.mailbox_max_deliveries_per_turn, 5);
        assert!(
            cfg.mailbox_reject_send_to_ended_session,
            "mailbox_reject_send_to_ended_session must default true (no-default-off rule)"
        );

        let rows: Vec<(String, serde_json::Value)> = vec![
            (
                "mailbox_delivery_ttl_minutes".to_owned(),
                serde_json::json!(120),
            ),
            (
                "mailbox_max_deliveries_per_turn".to_owned(),
                serde_json::json!(3),
            ),
            (
                "mailbox_reject_send_to_ended_session".to_owned(),
                serde_json::Value::Bool(false),
            ),
        ];
        super::apply_settings_overrides(&mut cfg, &rows);

        assert_eq!(cfg.mailbox_delivery_ttl_minutes, 120);
        assert_eq!(cfg.mailbox_max_deliveries_per_turn, 3);
        assert!(!cfg.mailbox_reject_send_to_ended_session);

        Ok(())
    }
}
