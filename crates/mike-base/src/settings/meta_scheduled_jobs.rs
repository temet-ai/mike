use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const LEARNING_PROMOTE_ENABLED_KEY: &str = "learning_promote.enabled";
pub const LEARNING_AUDIT_ENABLED_KEY: &str = "learning_audit.enabled";
pub const COMPRESSOR_LOOP_ENABLED_KEY: &str = "compressor_loop.enabled";
pub const LESSONS_COMMIT_ENABLED_KEY: &str = "lessons_commit.enabled";
pub const USAGE_AUDIT_ENABLED_KEY: &str = "usage_audit.enabled";
pub const LESSON_RELATIONS_ENABLED_KEY: &str = "lesson_relations_job.enabled";

pub fn enabled_entry(
    key: &'static str,
    label: &'static str,
    description: &'static str,
) -> SettingMeta {
    SettingMeta {
        key,
        group: "Scheduled jobs",
        kind: SettingKind::Bool,
        scope: SettingScope::Global,
        applies: SettingApplies::NextCycle,
        default: serde_json::Value::Bool(true),
        label,
        description,
        min_i64: None,
        max_i64: None,
        min_f64: None,
        max_f64: None,
        max_len: None,
        enum_variants: None,
    }
}

pub fn entries() -> Vec<SettingMeta> {
    vec![
        enabled_entry(
            LEARNING_PROMOTE_ENABLED_KEY,
            "Promote lessons into project skills",
            "Hourly at :30, from 20:30 to 03:30 UTC, run scripts/learning-promote from the \
            checkout this proxy was started in. It turns accepted lessons into project \
            memories and skills and writes them into the project checkouts on this machine, \
            so each machine runs its own. It makes no LLM call and sends nothing off the \
            machine. Read on every check, once a minute; a run already under way finishes.",
        ),
        enabled_entry(
            LEARNING_AUDIT_ENABLED_KEY,
            "Weekly learning audit",
            "On Mondays, once, between 01:00 and 06:59 UTC, run scripts/learning-audit \
            --weekly --sync from the checkout this proxy was started in. It audits the \
            preceding complete week with read-only queries, writes the report under \
            artifacts/learning-audit and publishes it through Mike's own project knowledge \
            ingestion. One machine per database runs it (the job lease). It makes no LLM \
            call and sends nothing off the machine. Read on every check, once a minute; a \
            run already under way finishes.",
        ),
        enabled_entry(
            COMPRESSOR_LOOP_ENABLED_KEY,
            "File-read compressor loop",
            "Daily, once between 04:30 and 06:59 UTC, run scripts/compressor-loop --days 7 \
            --apply from the checkout this proxy was started in. It scores the compressor's \
            forward and control arms and adjusts the file_read_compression settings through \
            the inspect API only when its evidence says so; it widens only when a current \
            qualification.json from the task trials is in its state folder. One machine per database runs it \
            (the job lease). It makes no LLM call and sends nothing off the machine. Read on \
            every check, once a minute; a run already under way finishes.",
        ),
        enabled_entry(
            LESSONS_COMMIT_ENABLED_KEY,
            "Commit synced lesson files",
            "Daily, once between 01:30 and 03:59 UTC (03:30 in Johannesburg), run \
            scripts/lessons-commit from the checkout this proxy was started in. It commits \
            the files under .claude/lessons that the knowledge collector wrote, only on \
            main and only while no other git operation is in progress. It never fetches or \
            pushes and makes no LLM call, so each machine runs its own. Read on every \
            check, once a minute; a run already under way finishes.",
        ),
        enabled_entry(
            USAGE_AUDIT_ENABLED_KEY,
            "Weekly usage audit",
            "On Sundays, once, between 06:00 and 10:59 UTC (08:00 in Johannesburg), run \
            scripts/usage-audit.sh from the checkout this proxy was started in. It reads the \
            capture database, writes a report under ~/.local/state/mike-audit and raises an \
            alarm (exit code 1, shown as the job's detail, not as a failure) when the cache \
            re-write rate, context size, dead steering rules or evaluation metrics look \
            wrong. One machine per database runs it (the job lease). It makes no LLM call \
            and sends nothing off the machine. Read on every check, once a minute; a run \
            already under way finishes.",
        ),
        enabled_entry(
            LESSON_RELATIONS_ENABLED_KEY,
            "Nightly lesson relation detector",
            "Daily, once between 22:00 and 23:59 UTC, run scripts/lesson-relations from the \
            checkout this proxy was started in. It runs `mike detect-lesson-relations`, which \
            sends pairs of lesson texts to the proposer and adjudicator models set under \
            Lesson relations, and only when lesson_relations.enabled is on. The nightly \
            lesson consolidation then retires the older lesson of each contradiction the \
            adjudicator agreed with. One machine per database runs it (the job lease). Read \
            on every check, once a minute; a run already under way finishes.",
        ),
    ]
}
