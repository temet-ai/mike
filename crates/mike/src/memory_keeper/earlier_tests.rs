use serde_json::json;
use uuid::Uuid;

use super::*;

fn hit(reference: &str, session: Uuid, score: f64) -> Hit {
    Hit {
        reference: reference.to_owned(),
        session,
        session_title: "Docker images".to_owned(),
        date: "2026-10-21".to_owned(),
        excerpt: "lets get the keycloak changes ready".to_owned(),
        score,
    }
}

#[test]
fn points_at_past_with_last_week_is_true() {
    assert!(points_at_past(
        "we had a ticket WPCW-1657 that we worked on last week"
    ));
    assert!(points_at_past("I thought we fixed this"));
    assert!(points_at_past("why is discord gone again?"));
}

#[test]
fn points_at_past_with_a_present_tense_task_is_false() {
    assert!(!points_at_past("add a retry to the embed worker"));
    assert!(!points_at_past("rename the againstall helper"));
}

#[test]
fn operator_words_with_a_reminder_drops_the_reminder() {
    let text =
        "fix the build<system-reminder>Lesson recorded from earlier sessions</system-reminder> now";
    assert_eq!(operator_words(text), "fix the build now");
    assert!(!points_at_past(&operator_words(text)));
}

#[test]
fn operator_words_with_an_unclosed_reminder_drops_the_rest() {
    assert_eq!(operator_words("keep<system-reminder>yesterday"), "keep");
}

#[test]
fn search_query_puts_identifiers_first_and_drops_filler() {
    let query =
        search_query("we had a ticket WPCW-1657 that we worked on last week regarding backstage");
    assert_eq!(query.as_deref(), Some("wpcw-1657 ticket backstage"));
}

#[test]
fn search_query_drops_links_and_long_numbers() {
    let query = search_query("https://gitlab.com/x/jobs/16032498283 the labs deploy failed");
    assert_eq!(query.as_deref(), Some("labs deploy failed"));
}

#[test]
fn search_query_with_only_filler_is_none() {
    assert_eq!(search_query("did we do that again?"), None);
}

#[test]
fn hits_from_keeps_turns_entries_and_notes_with_a_session() {
    let session = Uuid::from_u128(7);
    let page = json!({"items": [
        {"source_kind": "turn", "reference": "turn:1", "rerank_score": 1.5,
         "updated_at": "2026-10-21T08:00:00Z", "excerpt": "keycloak",
         "author": {"session_uuid": session.to_string(), "session_title": "Docker"}},
        {"source_kind": "lesson", "reference": "lesson:x", "rerank_score": 9.0,
         "author": {"session_uuid": session.to_string()}},
        {"source_kind": "turn", "reference": "turn:2", "rank": 0.1},
    ]});
    let hits = hits_from(&page);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].reference, "turn:1");
    assert_eq!(hits[0].date, "2026-10-21");
    assert!((hits[0].score - 1.5).abs() < f64::EPSILON);
}

#[test]
fn pick_drops_the_own_session_and_keeps_one_hit_per_session() {
    let own = Uuid::from_u128(1);
    let (a, b, c, d) = (
        Uuid::from_u128(2),
        Uuid::from_u128(3),
        Uuid::from_u128(4),
        Uuid::from_u128(5),
    );
    let picked = pick(
        vec![
            hit("turn:own", own, 9.0),
            hit("turn:a1", a, 3.0),
            hit("turn:a2", a, 2.9),
            hit("turn:b", b, 2.0),
            hit("turn:c", c, 1.0),
            hit("turn:d", d, 0.5),
        ],
        own,
    );
    let refs: Vec<&str> = picked.iter().map(|h| h.reference.as_str()).collect();
    assert_eq!(refs, ["turn:a1", "turn:b", "turn:c"]);
}

#[test]
fn render_with_hits_lists_each_with_its_reference_and_marker() -> anyhow::Result<()> {
    let block = render(&[hit("turn:42412", Uuid::from_u128(2), 1.0)])
        .ok_or_else(|| anyhow::anyhow!("no block"))?;
    assert!(block.text.contains(EARLIER_MARKER_OPEN));
    assert!(block.text.contains("[turn:42412]"));
    assert!(
        block
            .text
            .contains("2026-10-21, session 00000000 \"Docker images\"")
    );
    assert!(block.text.contains("read_project_knowledge"));
    Ok(())
}

#[test]
fn render_with_no_hits_is_none() {
    assert_eq!(render(&[]), None);
}

#[test]
fn render_same_hits_twice_gives_the_same_hash() {
    let hits = [hit("turn:1", Uuid::from_u128(2), 1.0)];
    assert_eq!(render(&hits).map(|b| b.hash), render(&hits).map(|b| b.hash));
}
