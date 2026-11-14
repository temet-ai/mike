use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

const LESSON_OUTCOME_MIN_JUDGED_KEY: &str = "lesson_outcome.min_judged_supplies";
const LESSON_OUTCOME_MIN_JUDGED_DEFAULT: i64 = 10;
const LESSON_OUTCOME_FLOOR_KEY: &str = "lesson_outcome.retire_floor";
const LESSON_OUTCOME_FLOOR_DEFAULT: f64 = 0.15;

pub fn entries() -> Vec<SettingMeta> {
    let mut v = turn_judge_entries();
    v.extend(session_judge_schedule_entries());
    v.extend(session_judge_content_entries());
    v.extend(lesson_outcome_entries());
    v
}

fn lesson_outcome_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: LESSON_OUTCOME_MIN_JUDGED_KEY,
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::json!(LESSON_OUTCOME_MIN_JUDGED_DEFAULT),
            label: "Judged supplies before a lesson can be retired",
            description: "The nightly consolidation retires a lesson from supply only after \
                the overnight verdicts cover at least this many sessions it was supplied to. \
                Read at the start of each nightly run.",
            min_i64: Some(1),
            max_i64: Some(10_000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: LESSON_OUTCOME_FLOOR_KEY,
            group: "Lesson worker",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::json!(LESSON_OUTCOME_FLOOR_DEFAULT),
            label: "Retirement floor (share followed)",
            description: "A lesson whose judged supplies were followed in less than this \
                share of sessions is retired from supply by the nightly consolidation. It \
                stays searchable and keeps its source corrections, and its page says why. \
                Set from the supply verdict counts, as there is no lesson holdout. Read at \
                the start of each nightly run.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn turn_judge_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "judge_worker.model",
            group: "Turn judge",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String("google/gemini-3.8-flash".to_owned()),
            label: "Model",
            description: "OpenRouter model the turn judge sends each closed turn to, or \
                codex/<model> to run it through the local Codex login instead. It must \
                support structured output. Clearing it falls back to the steerer's \
                default model.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_worker.system_prompt",
            group: "Turn judge",
            kind: SettingKind::Text,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String(super::judge_prompts::R3_PROMPT.to_owned()),
            label: "System prompt",
            description: "The rubric the turn judge scores against. It defines every \
                outcome label the judge may return, so a label removed from this text \
                is a label the judge can no longer produce. A prompt cannot be blank; \
                Reset to default restores the built-in rubric. A prompt that differs from \
                the built-in text is stamped on every verdict as the rubric id plus the \
                first 12 hex characters of its sha256, so a changed prompt is always \
                visible in the label history.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(16_384),
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_worker.batch_size",
            group: "Turn judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(50u64)),
            label: "Turns per sweep",
            description: "How many unjudged turns one sweep claims. Each turn is one \
                OpenRouter call, so this is the ceiling on what a single sweep costs.",
            min_i64: Some(1),
            max_i64: Some(500),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_worker.confidence_threshold",
            group: "Turn judge",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::from(0.5_f64),
            label: "Confidence threshold",
            description: "Verdicts the model returns below this confidence are recorded \
                as unknown instead of as the label the model picked.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "judge_worker.interval_secs",
            group: "Turn judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(60u64)),
            label: "Seconds between sweeps",
            description: "How long the turn judge waits between sweeps. The timer is \
                built once when the worker starts, so a change here needs a restart \
                while the settings above take effect on the next sweep.",
            min_i64: Some(10),
            max_i64: Some(86_400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn session_judge_schedule_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "session_judge.model",
            group: "Session judge",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String("google/gemini-3.5-flash-lite".to_owned()),
            label: "Model",
            description: "OpenRouter model the session judge sends each finished session \
                to, or codex/<model> to run it through the local Codex login instead. It \
                must support structured output; Anthropic models on OpenRouter reject that \
                request with an HTTP 400, so pick an OpenAI or Google model there. Changing \
                it re-judges sessions: a verdict is recorded against the model that \
                produced it, so sessions already scored by the previous model become \
                eligible again.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.system_prompt",
            group: "Session judge",
            kind: SettingKind::Text,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::String(
                super::judge_prompts::SESSION_JUDGE_SYSTEM_PROMPT.to_owned(),
            ),
            label: "System prompt",
            description: "The instructions the session judge runs: the outcome labels, \
                the meaning of each score band, and the shape of the two-paragraph \
                narrative overview that appears on the session page. A prompt cannot be \
                blank; Reset to default restores the built-in rubric.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(16_384),
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.batch_size",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(25u64)),
            label: "Sessions per poll",
            description: "How many sessions one poll judges. Each session is one \
                OpenRouter call. Raising it drains a backlog faster and costs \
                proportionally more per poll.",
            min_i64: Some(1),
            max_i64: Some(500),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.min_session_age_secs",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(60u64)),
            label: "Settled after (secs)",
            description: "How quiet a session must be before the judge will score it. \
                Too low and a session that is only pausing gets judged as if it had \
                ended.",
            min_i64: Some(10),
            max_i64: Some(86_400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn session_judge_content_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "session_judge.max_turns_in_prompt",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(40u64)),
            label: "Turns in the digest",
            description: "How many of the session's operator turns go into the digest \
                the model reads. The most recent ones are kept; anything older is left \
                out, so a long session is judged on its later turns.",
            min_i64: Some(1),
            max_i64: Some(500),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.max_chars_per_turn",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::NextCycle,
            default: serde_json::Value::Number(serde_json::Number::from(3_000i64)),
            label: "Max chars per turn",
            description: "Cap on how many characters of each turn's assistant response \
                text go into the session digest, kept from the end since that is where a \
                turn states what it delivered. A session with many long turns can \
                otherwise push hundreds of thousands of characters into one call \
                regardless of how few turns that is. The practical working range is \
                500 to 100,000; 0 disables the cap entirely.",
            min_i64: Some(0),
            max_i64: Some(100_000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.poll_interval_secs",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(120u64)),
            label: "Seconds between polls",
            description: "How long the session judge waits between polls. The timer is \
                built once when the worker starts, so a change here needs a restart \
                while the settings above take effect on the next poll.",
            min_i64: Some(10),
            max_i64: Some(86_400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "session_judge.request_timeout_secs",
            group: "Session judge",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(30u64)),
            label: "Request timeout (secs)",
            description: "Timeout for one session-judge call. The HTTP client is built \
                when the worker starts, so a change here needs a restart.",
            min_i64: Some(5),
            max_i64: Some(300),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}
