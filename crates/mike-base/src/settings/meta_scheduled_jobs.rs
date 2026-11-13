use super::{SettingApplies, SettingKind, SettingMeta, SettingScope};

pub const LEARNING_PROMOTE_ENABLED_KEY: &str = "learning_promote.enabled";
pub const LEARNING_AUDIT_ENABLED_KEY: &str = "learning_audit.enabled";
pub const COMPRESSOR_LOOP_ENABLED_KEY: &str = "compressor_loop.enabled";
pub const CARGO_SWEEP_ENABLED_KEY: &str = "cargo_sweep.enabled";
pub const LESSONS_COMMIT_ENABLED_KEY: &str = "lessons_commit.enabled";
pub const USAGE_AUDIT_ENABLED_KEY: &str = "usage_audit.enabled";
pub const LESSON_RELATIONS_ENABLED_KEY: &str = "lesson_relations_job.enabled";
pub const NIGHTLY_TESTS_ENABLED_KEY: &str = "nightly_tests.enabled";
pub const MEMORY_REVIEW_ENABLED_KEY: &str = "memory_review.enabled";

fn enabled_entry(key: &'static str, label: &'static str, description: &'static str) -> SettingMeta {
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
            CARGO_SWEEP_ENABLED_KEY,
            "Rust build folder sweep",
            "Every six hours, at 00:00, 06:00, 12:00 and 18:00 UTC, run \
            scripts/cargo-sweep-all from the checkout this proxy was started in. It deletes \
            superseded builds and idle build folders under this checkout's target folders and \
            ~/.cargo/build on this machine, so each machine runs its own. Read on every \
            check, once a minute; a run already under way finishes.",
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
        SettingMeta {
            default: serde_json::Value::Bool(false),
            ..enabled_entry(
                NIGHTLY_TESTS_ENABLED_KEY,
                "Nightly full Rust test run",
                "Daily, once between 02:00 and 04:59 UTC, run scripts/nightly-tests from the \
                checkout this proxy was started in. It updates a detached worktree of its own \
                (.claude/worktrees/nightly) to origin/main and runs make test-all there, every \
                Rust tier, because the pre-push hook runs only the tests of the areas a push \
                changed. Off by default: turn it on on the one machine that should carry the \
                run (omit or the Mac; the script refuses on aarch64 Linux). It makes no LLM \
                call and sends nothing off the machine. Read on every check, once a minute; a \
                run already under way finishes.",
            )
        },
        enabled_entry(
            MEMORY_REVIEW_ENABLED_KEY,
            "Daily memory review",
            "Daily, once between 05:15 and 07:59 UTC, run scripts/memory-review from the \
            checkout this proxy was started in. It starts a Claude Code session (claude -p, \
            Opus, file edits refused) through this proxy that reads what memory returned to \
            sessions since the last review, from the capture database and the mike tools, \
            and records what did not help and the fix on the memory-keeper work thread and \
            GitHub. Its instructions are the memory_review.brief setting. That session sends \
            captured session text to Anthropic like any other session. One machine per \
            database runs it (the job lease). Read on every check, once a minute; a run \
            already under way finishes.",
        ),
    ]
}
