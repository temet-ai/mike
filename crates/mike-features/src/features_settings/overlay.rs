use serde_json::Value;

use super::meta_shell_output::{
    DEFAULT_GRACE_TURNS, DEFAULT_HEAD_LINES, DEFAULT_MIN_RESULT_BYTES,
    DEFAULT_SIGNAL_CONTEXT_LINES, DEFAULT_SIGNAL_LINE_CAP, DEFAULT_TAIL_LINES,
};
use super::sections::weasel_intercept::DEFAULT_CORRECTION_PROMPT;
use crate::settings::overlay_apply::{
    set_bool, set_prompt, set_string, set_string_list, set_usize,
};
use crate::settings::{FeatureOverlays, OverlaySection};

pub const DEFAULT_PROTECTED_PATHS: &[&str] = &[
    "**/CLAUDE.md",
    "**/BRIEF*.md",
    "**/.claude/**",
    "**/docs/plans/**",
    "**/artifacts/threads/**",
    "**/*.rules.md",
];

#[derive(Debug, Clone, PartialEq)]
pub struct RemindersOverlay {
    pub inject_lessons: bool,
    pub max_lessons_to_inject: usize,
}

impl Default for RemindersOverlay {
    fn default() -> Self {
        Self {
            inject_lessons: false,
            max_lessons_to_inject: 5,
        }
    }
}

impl OverlaySection for RemindersOverlay {}

#[derive(Debug, Clone, PartialEq)]
pub struct WeaselOverlay {
    pub forbidden_words: Vec<String>,
    pub correction_prompt: String,
}

impl Default for WeaselOverlay {
    fn default() -> Self {
        Self {
            forbidden_words: Vec::new(),
            correction_prompt: DEFAULT_CORRECTION_PROMPT.to_owned(),
        }
    }
}

impl OverlaySection for WeaselOverlay {}

#[derive(Debug, Clone, PartialEq)]
pub struct FileReadCompressionOverlay {
    pub min_bytes: usize,
    pub head_lines: usize,
    pub tail_lines: usize,
    pub enabled_producers: Vec<String>,
    pub protected_paths: Vec<String>,
    pub grace_turns: usize,
    pub mode: String,
    pub forward_share: usize,
}

impl Default for FileReadCompressionOverlay {
    fn default() -> Self {
        Self {
            min_bytes: 1500,
            head_lines: 40,
            tail_lines: 20,
            enabled_producers: vec!["Read".to_owned()],
            protected_paths: DEFAULT_PROTECTED_PATHS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            grace_turns: 3,
            mode: "shadow".to_owned(),
            forward_share: 0,
        }
    }
}

impl FileReadCompressionOverlay {
    pub fn forward(&self) -> bool {
        self.mode == "forward"
    }
}

impl OverlaySection for FileReadCompressionOverlay {}

#[derive(Debug, Clone, PartialEq)]
pub struct ShellOutputCompressionOverlay {
    pub min_bytes: usize,
    pub head_lines: usize,
    pub tail_lines: usize,
    pub signal_context_lines: usize,
    pub signal_line_cap: usize,
    pub grace_turns: usize,
    pub mode: String,
    pub forward_share: usize,
}

impl Default for ShellOutputCompressionOverlay {
    fn default() -> Self {
        Self {
            min_bytes: DEFAULT_MIN_RESULT_BYTES,
            head_lines: DEFAULT_HEAD_LINES,
            tail_lines: DEFAULT_TAIL_LINES,
            signal_context_lines: DEFAULT_SIGNAL_CONTEXT_LINES,
            signal_line_cap: DEFAULT_SIGNAL_LINE_CAP,
            grace_turns: DEFAULT_GRACE_TURNS,
            mode: "shadow".to_owned(),
            forward_share: 0,
        }
    }
}

impl ShellOutputCompressionOverlay {
    pub fn forward(&self) -> bool {
        self.mode == "forward"
    }
}

impl OverlaySection for ShellOutputCompressionOverlay {}

#[derive(Debug, Clone, PartialEq)]
pub struct ContextShapingOverlay {
    pub compression_dedup_enabled: bool,
    pub context_focus_question_mode: String,
}

impl Default for ContextShapingOverlay {
    fn default() -> Self {
        Self {
            compression_dedup_enabled: false,
            context_focus_question_mode: "plain".to_owned(),
        }
    }
}

impl OverlaySection for ContextShapingOverlay {}

pub fn apply_overlay(overlays: &mut FeatureOverlays, key: &str, value: &Value) -> bool {
    if let Some(field) = key.strip_prefix("file_read_compression.") {
        return apply_file_read_row(overlays, field, key, value);
    }
    if let Some(field) = key.strip_prefix("shell_output_compression.") {
        return apply_shell_output_row(overlays, field, key, value);
    }
    match key {
        "reminders.inject_lessons" => overlays.update::<RemindersOverlay>(|o| {
            set_bool(&mut o.inject_lessons, key, value);
        }),
        "reminders.max_lessons_to_inject" => overlays.update::<RemindersOverlay>(|o| {
            set_usize(&mut o.max_lessons_to_inject, key, value);
        }),
        "weasel_intercept.forbidden_words" => overlays.update::<WeaselOverlay>(|o| {
            set_string_list(&mut o.forbidden_words, key, value);
        }),
        "weasel_intercept.correction_prompt" => overlays.update::<WeaselOverlay>(|o| {
            set_prompt(&mut o.correction_prompt, key, value);
        }),
        "compression_dedup.enabled" => overlays.update::<ContextShapingOverlay>(|o| {
            set_bool(&mut o.compression_dedup_enabled, key, value);
        }),
        "context_focus_question.mode" => overlays.update::<ContextShapingOverlay>(|o| {
            set_string(&mut o.context_focus_question_mode, key, value);
        }),
        _ => return false,
    }
    true
}

fn apply_file_read_row(
    overlays: &mut FeatureOverlays,
    field: &str,
    key: &str,
    value: &Value,
) -> bool {
    let mut handled = true;
    overlays.update::<FileReadCompressionOverlay>(|o| match field {
        "min_bytes" => set_usize(&mut o.min_bytes, key, value),
        "head_lines" => set_usize(&mut o.head_lines, key, value),
        "tail_lines" => set_usize(&mut o.tail_lines, key, value),
        "enabled_producers" => set_string_list(&mut o.enabled_producers, key, value),
        "protected_paths" => set_string_list(&mut o.protected_paths, key, value),
        "grace_turns" => set_usize(&mut o.grace_turns, key, value),
        "mode" => set_string(&mut o.mode, key, value),
        "forward_share" => set_usize(&mut o.forward_share, key, value),
        _ => handled = false,
    });
    handled
}

fn apply_shell_output_row(
    overlays: &mut FeatureOverlays,
    field: &str,
    key: &str,
    value: &Value,
) -> bool {
    let mut handled = true;
    overlays.update::<ShellOutputCompressionOverlay>(|o| match field {
        "min_bytes" => set_usize(&mut o.min_bytes, key, value),
        "head_lines" => set_usize(&mut o.head_lines, key, value),
        "tail_lines" => set_usize(&mut o.tail_lines, key, value),
        "signal_context_lines" => set_usize(&mut o.signal_context_lines, key, value),
        "signal_line_cap" => set_usize(&mut o.signal_line_cap, key, value),
        "grace_turns" => set_usize(&mut o.grace_turns, key, value),
        "forward_share" => set_usize(&mut o.forward_share, key, value),
        "mode" => set_string(&mut o.mode, key, value),
        _ => handled = false,
    });
    handled
}
