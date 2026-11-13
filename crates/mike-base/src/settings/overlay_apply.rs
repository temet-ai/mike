use super::SettingsOverlay;

pub fn apply_db_rows_to_overlay(rows: Vec<(String, serde_json::Value)>) -> SettingsOverlay {
    let mut overlay = SettingsOverlay::default();
    for (key, value) in rows {
        apply_one_row(&mut overlay, &key, &value);
    }
    overlay
}

fn apply_one_row(overlay: &mut SettingsOverlay, key: &str, value: &serde_json::Value) {
    if super::overlay_apply_provider::apply_provider_row(overlay, key, value) {
        return;
    }
    if super::overlay_apply_session_end::apply_session_end_row(overlay, key, value) {
        return;
    }
    if super::overlay_apply_turn_judge::apply_turn_judge_row(overlay, key, value) {
        return;
    }
    if super::overlay_apply_decisions::apply_decisions_row(overlay, key, value) {
        return;
    }
    if apply_session_scoped_row(overlay, key, value) {
        return;
    }
    if apply_worker_identity_row(overlay, key, value) {
        return;
    }
    apply_global_llm_row(overlay, key, value);
}

fn apply_session_scoped_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "reminders.inject_lessons" => set_bool(&mut overlay.reminders_inject_lessons, key, value),
        "reminders.max_lessons_to_inject" => {
            set_usize(&mut overlay.reminders_max_lessons_to_inject, key, value);
        }
        "weasel_intercept.forbidden_words" => {
            set_string_list(&mut overlay.weasel_intercept_forbidden_words, key, value);
        }
        "weasel_intercept.correction_prompt" => {
            set_prompt(&mut overlay.weasel_intercept_correction_prompt, key, value);
        }
        "recall_injector.max_lessons_per_topic" => {
            set_usize(
                &mut overlay.recall_injector_max_lessons_per_topic,
                key,
                value,
            );
        }
        "file_read_compression.min_bytes" => {
            set_usize(&mut overlay.file_read_compression_min_bytes, key, value);
        }
        "file_read_compression.head_lines" => {
            set_usize(&mut overlay.file_read_compression_head_lines, key, value);
        }
        "file_read_compression.tail_lines" => {
            set_usize(&mut overlay.file_read_compression_tail_lines, key, value);
        }
        "file_read_compression.enabled_producers" => {
            set_string_list(
                &mut overlay.file_read_compression_enabled_producers,
                key,
                value,
            );
        }
        "file_read_compression.protected_paths" => {
            set_string_list(
                &mut overlay.file_read_compression_protected_paths,
                key,
                value,
            );
        }
        "file_read_compression.grace_turns" => {
            set_usize(&mut overlay.file_read_compression_grace_turns, key, value);
        }
        "file_read_compression.mode" => {
            set_string(&mut overlay.file_read_compression_mode, key, value);
        }
        "file_read_compression.forward_share" => {
            set_usize(&mut overlay.file_read_compression_forward_share, key, value);
        }
        key if key.starts_with("shell_output_compression.") => {
            return apply_shell_output_row(overlay, key, value);
        }
        "compression_dedup.enabled" => {
            set_bool(&mut overlay.compression_dedup_enabled, key, value);
        }
        "context_focus_question.mode" => {
            set_string(&mut overlay.context_focus_question_mode, key, value);
        }
        _ => return false,
    }
    true
}

fn apply_shell_output_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    let target = match key {
        "shell_output_compression.min_bytes" => &mut overlay.shell_output_compression_min_bytes,
        "shell_output_compression.head_lines" => &mut overlay.shell_output_compression_head_lines,
        "shell_output_compression.tail_lines" => &mut overlay.shell_output_compression_tail_lines,
        "shell_output_compression.signal_context_lines" => {
            &mut overlay.shell_output_compression_signal_context_lines
        }
        "shell_output_compression.signal_line_cap" => {
            &mut overlay.shell_output_compression_signal_line_cap
        }
        "shell_output_compression.grace_turns" => &mut overlay.shell_output_compression_grace_turns,
        "shell_output_compression.forward_share" => {
            &mut overlay.shell_output_compression_forward_share
        }
        "shell_output_compression.mode" => {
            set_string(&mut overlay.shell_output_compression_mode, key, value);
            return true;
        }
        _ => return false,
    };
    set_usize(target, key, value);
    true
}

fn apply_worker_identity_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "judge_worker.model" => set_optional_string(&mut overlay.judge_worker_model, key, value),
        "judge_worker.system_prompt" => {
            set_optional_prompt(&mut overlay.judge_worker_system_prompt, key, value);
        }
        "judge_worker.batch_size" => {
            set_optional_i64(&mut overlay.judge_worker_batch_size, key, value);
        }
        "judge_worker.confidence_threshold" => {
            set_optional_f32(&mut overlay.judge_worker_confidence_threshold, key, value);
        }
        "session_judge.model" => set_optional_string(&mut overlay.session_judge_model, key, value),
        "session_judge.system_prompt" => {
            set_optional_prompt(&mut overlay.session_judge_system_prompt, key, value);
        }
        "session_judge.batch_size" => {
            set_optional_i64(&mut overlay.session_judge_batch_size, key, value);
        }
        "session_judge.min_session_age_secs" => {
            set_optional_u64(&mut overlay.session_judge_min_session_age_secs, key, value);
        }
        "session_judge.max_turns_in_prompt" => {
            set_optional_usize(&mut overlay.session_judge_max_turns_in_prompt, key, value);
        }
        "session_judge.max_chars_per_turn" => {
            set_i64(&mut overlay.session_judge_max_chars_per_turn, key, value);
        }
        _ => return false,
    }
    true
}

fn apply_global_llm_row(
    overlay: &mut SettingsOverlay,
    key: &str,
    value: &serde_json::Value,
) -> bool {
    match key {
        "health.llm_failures_degrade_status" => {
            set_bool(&mut overlay.health_llm_failures_degrade_status, key, value);
        }
        "judge_worker.reasoning_effort" => {
            set_string(&mut overlay.judge_worker_reasoning_effort, key, value);
        }
        "judge_worker.max_output_tokens" => {
            set_i64(&mut overlay.judge_worker_max_output_tokens, key, value);
        }
        "session_judge.reasoning_effort" => {
            set_string(&mut overlay.session_judge_reasoning_effort, key, value);
        }
        "session_judge.max_output_tokens" => {
            set_i64(&mut overlay.session_judge_max_output_tokens, key, value);
        }
        "llm.reasoning_effort" => set_string(&mut overlay.llm_reasoning_effort, key, value),
        "llm.max_output_tokens" => set_i64(&mut overlay.llm_max_output_tokens, key, value),
        "llm.rate_limit_per_minute" => {
            set_i64(&mut overlay.llm_rate_limit_per_minute, key, value);
        }
        "llm.ceiling_max_calls" => set_i64(&mut overlay.llm_ceiling_max_calls, key, value),
        "llm.ceiling_window_secs" => set_i64(&mut overlay.llm_ceiling_window_secs, key, value),
        "llm.daily_spend_ceiling_usd" => {
            set_f64(&mut overlay.llm_daily_spend_ceiling_usd, key, value);
        }
        "llm.spend_alert_fraction" => set_f64(&mut overlay.llm_spend_alert_fraction, key, value),
        "llm.record_usage" => set_bool(&mut overlay.llm_record_usage, key, value),
        "llm.budget.steerer.share_pct" => {
            set_i64(&mut overlay.llm_budget_steerer_share_pct, key, value);
        }
        "llm.budget.session_judge.share_pct" => {
            set_i64(&mut overlay.llm_budget_session_judge_share_pct, key, value);
        }
        "llm.budget.judge_worker.share_pct" => {
            set_i64(&mut overlay.llm_budget_judge_worker_share_pct, key, value);
        }
        "llm.budget.lesson_relations.share_pct" => {
            set_i64(
                &mut overlay.llm_budget_lesson_relations_share_pct,
                key,
                value,
            );
        }
        "llm.budget.lesson_proposer.share_pct" => {
            set_i64(
                &mut overlay.llm_budget_lesson_proposer_share_pct,
                key,
                value,
            );
        }
        "llm.budget.eval.share_pct" => {
            set_i64(&mut overlay.llm_budget_eval_share_pct, key, value);
        }
        "llm.budget.other.share_pct" => {
            set_i64(&mut overlay.llm_budget_other_share_pct, key, value);
        }
        "steerer.model" => set_optional_string(&mut overlay.steerer_model, key, value),
        "steerer.reasoning_effort" => set_string(&mut overlay.steerer_reasoning_effort, key, value),
        "steerer.max_output_tokens" => {
            set_i64(&mut overlay.steerer_max_output_tokens, key, value);
        }
        "steerer.retain_pass_verdicts" => {
            set_bool(&mut overlay.steerer_retain_pass_verdicts, key, value);
        }
        "steerer.session_note_limit" => {
            set_i64(&mut overlay.steerer_session_note_limit, key, value);
        }
        "steerer.session_note_window_minutes" => {
            set_i64(&mut overlay.steerer_session_note_window_minutes, key, value);
        }
        "steerer.wrong_note_mute_hours" => {
            set_i64(&mut overlay.steerer_wrong_note_mute_hours, key, value);
        }
        "steerer.verifier.input_budget" => {
            set_string(&mut overlay.steerer_verifier_input_budget, key, value);
        }
        "steerer.intake_criteria.input_budget" => {
            set_string(
                &mut overlay.steerer_intake_criteria_input_budget,
                key,
                value,
            );
        }
        _ => return false,
    }
    true
}

fn set_bool(field: &mut bool, key: &str, value: &serde_json::Value) {
    if let Some(b) = value.as_bool() {
        *field = b;
    } else {
        tracing::warn!(
            key,
            "settings: expected bool for {key}, got {value}; keeping default"
        );
    }
}

fn set_usize(field: &mut usize, key: &str, value: &serde_json::Value) {
    if let Some(n) = value.as_u64() {
        *field = n as usize;
    } else {
        tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        );
    }
}

fn set_i64(field: &mut i64, key: &str, value: &serde_json::Value) {
    if let Some(n) = value.as_i64() {
        *field = n;
    } else {
        tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        );
    }
}

fn set_f64(field: &mut f64, key: &str, value: &serde_json::Value) {
    if let Some(f) = value.as_f64() {
        *field = f;
    } else {
        tracing::warn!(
            key,
            "settings: expected float for {key}, got {value}; keeping default"
        );
    }
}

fn set_string(field: &mut String, key: &str, value: &serde_json::Value) {
    if let Some(s) = value.as_str() {
        *field = s.to_owned();
    } else {
        tracing::warn!(
            key,
            "settings: expected string for {key}, got {value}; keeping default"
        );
    }
}

fn warn_expected_string(key: &str, value: &serde_json::Value) {
    tracing::warn!(
        key,
        "settings: expected string for {key}, got {value}; keeping default"
    );
}

fn warn_prompt_blank(key: &str) {
    tracing::warn!(
        key,
        "settings: stored prompt for {key} is blank, which a prompt may not be; keeping the built-in prompt"
    );
}

fn set_prompt(field: &mut String, key: &str, value: &serde_json::Value) {
    let Some(s) = value.as_str() else {
        warn_expected_string(key, value);
        return;
    };
    if s.trim().is_empty() {
        warn_prompt_blank(key);
        return;
    }
    *field = s.to_owned();
}

fn set_string_list(field: &mut Vec<String>, key: &str, value: &serde_json::Value) {
    if let Some(arr) = value.as_array() {
        *field = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    } else {
        tracing::warn!(
            key,
            "settings: expected array for {key}, got {value}; keeping default"
        );
    }
}

fn set_optional_string(field: &mut Option<String>, key: &str, value: &serde_json::Value) {
    if let Some(s) = value.as_str() {
        *field = Some(s.to_owned());
    } else {
        tracing::warn!(
            key,
            "settings: expected string for {key}, got {value}; keeping default"
        );
    }
}

fn set_optional_prompt(field: &mut Option<String>, key: &str, value: &serde_json::Value) {
    let Some(s) = value.as_str() else {
        warn_expected_string(key, value);
        return;
    };
    if s.trim().is_empty() {
        warn_prompt_blank(key);
        *field = None;
        return;
    }
    *field = Some(s.to_owned());
}

pub fn set_optional_i64(field: &mut Option<i64>, key: &str, value: &serde_json::Value) {
    if let Some(n) = value.as_i64() {
        *field = Some(n);
    } else {
        tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        );
    }
}

fn set_optional_f32(field: &mut Option<f32>, key: &str, value: &serde_json::Value) {
    if let Some(f) = value.as_f64() {
        *field = Some(f as f32);
    } else {
        tracing::warn!(
            key,
            "settings: expected float for {key}, got {value}; keeping default"
        );
    }
}

fn set_optional_usize(field: &mut Option<usize>, key: &str, value: &serde_json::Value) {
    if let Some(n) = value.as_u64() {
        *field = Some(n as usize);
    } else {
        tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        );
    }
}

fn set_optional_u64(field: &mut Option<u64>, key: &str, value: &serde_json::Value) {
    if let Some(n) = value.as_u64() {
        *field = Some(n);
    } else {
        tracing::warn!(
            key,
            "settings: expected int for {key}, got {value}; keeping default"
        );
    }
}

#[cfg(test)]
#[path = "overlay_apply_tests.rs"]
mod tests;
