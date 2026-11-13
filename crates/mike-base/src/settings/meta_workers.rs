use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

const LESSON_OUTCOME_MIN_JUDGED_KEY: &str = "lesson_outcome.min_judged_supplies";
const LESSON_OUTCOME_MIN_JUDGED_DEFAULT: i64 = 10;
const LESSON_OUTCOME_FLOOR_KEY: &str = "lesson_outcome.retire_floor";
const LESSON_OUTCOME_FLOOR_DEFAULT: f64 = 0.15;

pub fn entries() -> Vec<SettingMeta> {
    let mut v = turn_judge_entries();
    v.extend(session_judge_schedule_entries());
    v.extend(session_judge_content_entries());
    v.extend(lesson_worker_entries());
    v.extend(lesson_relations_entries());
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

fn lesson_worker_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "lesson_worker.dedup_threshold",
            group: "Lesson worker",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::json!(0.85),
            label: "Dedup threshold",
            description: "Cosine similarity threshold above which two lessons are considered duplicates.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.max_lessons_per_project",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(500i64)),
            label: "Max lessons per project",
            description: "Maximum active lessons retained per project after GC.",
            min_i64: Some(10),
            max_i64: Some(10_000),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.session_consolidation_idle_secs",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(300u64)),
            label: "Session consolidation idle (secs)",
            description: "Seconds of inactivity before a session's lessons are consolidated.",
            min_i64: Some(60),
            max_i64: Some(86400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.nightly_consolidation_hour",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(3u64)),
            label: "Nightly consolidation hour",
            description: "Local hour (0–23) at which the nightly lesson consolidation runs.",
            min_i64: Some(0),
            max_i64: Some(23),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_worker.gc_interval_secs",
            group: "Lesson worker",
            kind: SettingKind::Int,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Number(serde_json::Number::from(3600u64)),
            label: "GC interval (secs)",
            description: "How often the lesson GC sweeps old entries.",
            min_i64: Some(60),
            max_i64: Some(86400),
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
    ]
}

fn lesson_relations_entries() -> Vec<SettingMeta> {
    vec![
        SettingMeta {
            key: "lesson_relations.enabled",
            group: "Lesson relations",
            kind: SettingKind::Bool,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::Bool(true),
            label: "Lesson relations: enabled",
            description: "Enable the `mike detect-lesson-relations` detector, which proposes \
                supersedes/refines/recurs/contradicts relations between lessons and runs \
                nightly as the lesson_relations scheduled job. When the adjudicator agrees \
                with a proposed contradiction, the nightly consolidation confirms it and \
                retires the older lesson from supply as superseded; it stays searchable and \
                nothing is deleted.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: None,
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.proposer_model",
            group: "Lesson relations",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::String("openai/gpt-5.6-luna".to_owned()),
            label: "Proposer model",
            description: "OpenRouter model, or codex/<model>, that proposes a relation from an \
                evidence bundle. Must differ from the adjudicator model — a model never \
                adjudicates its own proposal.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.adjudicator_model",
            group: "Lesson relations",
            kind: SettingKind::String,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::Value::String("google/gemini-3.5-flash-lite".to_owned()),
            label: "Adjudicator model",
            description: "OpenRouter model, or codex/<model>, that independently reviews the \
                proposer's claim against the same evidence. Must differ from the proposer model.",
            min_i64: None,
            max_i64: None,
            min_f64: None,
            max_f64: None,
            max_len: Some(256),
            enum_variants: None,
        },
        SettingMeta {
            key: "lesson_relations.candidate_distance_threshold",
            group: "Lesson relations",
            kind: SettingKind::Float,
            scope: SettingScope::Global,
            applies: SettingApplies::OnRestart,
            default: serde_json::json!(0.2),
            label: "Candidate distance threshold",
            description: "Cosine distance below which a same-project lesson pair is nominated \
                as a relation candidate. Nomination only — kind, direction, and status always \
                come from evidence, never from this threshold alone.",
            min_i64: None,
            max_i64: None,
            min_f64: Some(0.0),
            max_f64: Some(1.0),
            max_len: None,
            enum_variants: None,
        },
    ]
}
