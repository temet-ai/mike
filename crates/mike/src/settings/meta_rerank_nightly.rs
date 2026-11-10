use serde_json::{Value, json};

use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};
use crate::analysis::rerank::{
    DEFAULT_COLD_TIMEOUT_MS, DEFAULT_MIN_QUERY_WORDS, DEFAULT_RESCUE_MIN_KEPT,
    DEFAULT_RESCUE_SCORE, RerankNightly,
};

const SEARCH_GROUP: &str = "Reranker";
const NIGHT_GROUP: &str = "Overnight reranking";

struct Entry {
    key: &'static str,
    group: &'static str,
    applies: SettingApplies,
    default: Value,
    label: &'static str,
    description: &'static str,
}

fn int(entry: Entry, min: i64, max: i64) -> SettingMeta {
    meta(
        entry,
        SettingKind::Int,
        (Some(min), Some(max)),
        (None, None),
    )
}

fn float(entry: Entry, min: f64, max: f64) -> SettingMeta {
    meta(
        entry,
        SettingKind::Float,
        (None, None),
        (Some(min), Some(max)),
    )
}

fn meta(
    entry: Entry,
    kind: SettingKind,
    (min_i64, max_i64): (Option<i64>, Option<i64>),
    (min_f64, max_f64): (Option<f64>, Option<f64>),
) -> SettingMeta {
    SettingMeta {
        key: entry.key,
        group: entry.group,
        kind,
        scope: SettingScope::Global,
        applies: entry.applies,
        default: entry.default,
        label: entry.label,
        description: entry.description,
        min_i64,
        max_i64,
        min_f64,
        max_f64,
        max_len: None,
        enum_variants: None,
    }
}

fn night(
    key: &'static str,
    default: Value,
    label: &'static str,
    description: &'static str,
) -> Entry {
    Entry {
        key,
        group: NIGHT_GROUP,
        applies: SettingApplies::NextCycle,
        default,
        label,
        description,
    }
}

pub(super) fn entries() -> Vec<SettingMeta> {
    let mut entries = search_entries();
    entries.extend(night_entries());
    entries
}

fn search_entries() -> Vec<SettingMeta> {
    vec![
        float(
            Entry {
                key: "rerank.rescue_score",
                group: SEARCH_GROUP,
                applies: SettingApplies::NextMessage,
                default: json!(DEFAULT_RESCUE_SCORE),
                label: "Rescue score",
                description: "A result whose embedding score is under the model's floor, or that matches only some query words, stays in a reranked search when the reranker scores it at least this high. Raise it to drop more weak matches; lower it to keep more.",
            },
            -10.0,
            10.0,
        ),
        int(
            Entry {
                key: "rerank.rescue_min_kept",
                group: SEARCH_GROUP,
                applies: SettingApplies::NextMessage,
                default: json!(DEFAULT_RESCUE_MIN_KEPT),
                label: "Results kept whatever their score",
                description: "A reranked search always keeps this many of its best-scored results, so a search with real candidates never comes back empty.",
            },
            0,
            20,
        ),
        int(
            Entry {
                key: "rerank.cold_timeout_ms",
                group: SEARCH_GROUP,
                applies: SettingApplies::NextMessage,
                default: json!(DEFAULT_COLD_TIMEOUT_MS),
                label: "Reranker cold-start timeout (ms)",
                description: "How long a search waits for a reranker that is loading its model, or that may have unloaded since the last search, before it returns its plain order. A search never waits longer than this plus the reranker timeout in total.",
            },
            1_000,
            30_000,
        ),
        int(
            Entry {
                key: "rerank.min_query_words",
                group: SEARCH_GROUP,
                applies: SettingApplies::NextMessage,
                default: json!(DEFAULT_MIN_QUERY_WORDS),
                label: "Fewest words in a reranked query",
                description: "A query with fewer words than this keeps its plain order and is not reranked. 1 reranks every query.",
            },
            1,
            20,
        ),
        meta(
            Entry {
                key: "rerank.search_no_project",
                group: SEARCH_GROUP,
                applies: SettingApplies::NextMessage,
                default: json!(true),
                label: "Also search sessions with no project",
                description: "A reranked search also draws candidates from sessions started outside any project, such as in the home folder, and the reranker ranks both together.",
            },
            SettingKind::Bool,
            (None, None),
            (None, None),
        ),
    ]
}

fn night_entries() -> Vec<SettingMeta> {
    let d = RerankNightly::default();
    vec![
        int(
            night(
                "rerank.nightly.start_hour_utc",
                json!(d.start_hour_utc),
                "Overnight window start (UTC hour)",
                "Hour of the day, in UTC, from which the overnight reranker pass may run.",
            ),
            0,
            23,
        ),
        int(
            night(
                "rerank.nightly.end_hour_utc",
                json!(d.end_hour_utc),
                "Overnight window end (UTC hour)",
                "Hour of the day, in UTC, at which the overnight pass stops starting. The same start and end hour means it never runs on its own.",
            ),
            0,
            23,
        ),
        float(
            night(
                "rerank.nightly.max_load_per_cpu",
                json!(d.max_load_per_cpu),
                "Load ceiling per CPU",
                "The pass starts, and keeps grading, only while the one-minute load average divided by the CPU count is at or under this.",
            ),
            0.0,
            4.0,
        ),
        int(
            night(
                "rerank.tuning.lookback_days",
                json!(d.lookback_days),
                "Tuning lookback (days)",
                "How many days of graded searches the tuner learns from.",
            ),
            1,
            90,
        ),
        int(
            night(
                "rerank.tuning.min_graded_searches",
                json!(d.min_graded_searches),
                "Graded searches before tuning",
                "The tuner changes nothing until at least this many graded searches are inside the lookback.",
            ),
            1,
            10_000,
        ),
        float(
            night(
                "rerank.tuning.min_gain",
                json!(d.min_gain),
                "Smallest gain kept",
                "A ranking change is kept only when the share of used results in the top five rises by at least this much.",
            ),
            0.0,
            1.0,
        ),
        float(
            night(
                "rerank.lesson_pairs.min_score",
                json!(d.lesson_pair_min_score),
                "Lesson pair score",
                "Two lessons go to the relation detector first when the reranker scores them at least this close.",
            ),
            -10.0,
            10.0,
        ),
        int(
            night(
                "rerank.related.per_thread",
                json!(d.related_per_thread),
                "Related items kept per thread",
                "How many reranked related items the overnight pass keeps for each work thread's card.",
            ),
            1,
            50,
        ),
    ]
}
