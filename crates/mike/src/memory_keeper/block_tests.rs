use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use super::super::resolve::{Pick, ThreadFacts};
use super::super::snapshot::{ProjectThreads, ThreadEntry};
use super::*;

fn entry(slug: &str, goal: &str) -> ThreadEntry {
    ThreadEntry {
        thread_id: 1,
        facts: ThreadFacts {
            slug: slug.to_owned(),
            title: format!("{slug} title"),
            goal: goal.to_owned(),
            status: "implementing".to_owned(),
            branch: None,
            worktree: None,
            last_session_uuid: None,
            last_checkpoint_at: DateTime::<Utc>::UNIX_EPOCH,
            linked_sessions: BTreeSet::new(),
            checkout_dirs: BTreeSet::new(),
        },
        text: Arc::from(format!("Work thread `{slug}`\nNext up: x [thread:{slug}]").as_str()),
        content_hash: "h".to_owned(),
    }
}

fn pick(slug: &str) -> Pick {
    Pick {
        slug: slug.to_owned(),
        score: 1.0,
        signals: Vec::new(),
    }
}

#[test]
fn render_one_wraps_the_card_unchanged_after_the_marker() {
    let threads = ProjectThreads {
        threads: vec![entry("alpha", "g")],
    };
    let Some(block) = render(&threads, &Resolution::One(pick("alpha"))) else {
        panic!("expected a block");
    };
    let expected_body = "Work thread `alpha`\nNext up: x [thread:alpha]";
    assert_eq!(
        block.text,
        format!(
            "<system-reminder source=\"memory-keeper\">\n[keeper:{}]\n{expected_body}\n</system-reminder>",
            block.hash
        )
    );
    assert_eq!(block.hash.len(), 12);
}

#[test]
fn render_several_falls_back_to_title_and_clips_long_goals() {
    let long_goal = "word ".repeat(60);
    let threads = ProjectThreads {
        threads: vec![entry("alpha", ""), entry("beta", &long_goal)],
    };
    let Some(block) = render(
        &threads,
        &Resolution::Several(vec![pick("alpha"), pick("beta")]),
    ) else {
        panic!("expected a block");
    };
    assert!(
        block
            .text
            .contains("- alpha (implementing): alpha title [thread:alpha]")
    );
    let beta = block
        .text
        .lines()
        .find(|line| line.starts_with("- beta"))
        .unwrap_or_default();
    assert!(beta.contains('…'));
    assert!(block.text.contains("work_thread_get <slug>"));
}

#[test]
fn render_none_and_unknown_slug_give_no_block() {
    let threads = ProjectThreads {
        threads: vec![entry("alpha", "g")],
    };
    assert!(render(&threads, &Resolution::None).is_none());
    assert!(render(&threads, &Resolution::One(pick("ghost"))).is_none());
}
