use super::{card_hashes, chosen_score, read_evidence, remember_scores};
use crate::lesson_worker::LessonSnapshot;

#[test]
fn read_evidence_names_each_lesson_route_and_splits_client_cards_from_proxy_cards() {
    let original = concat!(
        r#"{"messages":[{"content":"Lessons recorded from earlier sessions in this project:\n"#,
        r#"[lesson:12] keep it narrow\n[lesson:7] no stash\" , "x":"#,
        r#""Lesson recorded from earlier sessions in this project [lesson:9] tether line ","#,
        r#""[keeper:aaaaaaaaaaaa]\ncard"}]}"#
    );
    let sent = format!("{original} [keeper:aaaaaaaaaaaa] [keeper:bbbbbbbbbbbb] [keeper:nothex]");
    let evidence = read_evidence("s1", original.as_bytes(), sent.as_bytes(), &[3, 12]);
    let routes: Vec<_> = evidence
        .lessons
        .iter()
        .map(|l| (l.lesson_id, l.route))
        .collect();
    assert_eq!(
        routes,
        vec![(3, "proxy"), (12, "proxy"), (7, "mod"), (9, "tether")]
    );
    let cards: Vec<_> = evidence
        .cards
        .iter()
        .map(|c| (c.hash.as_str(), c.added_by_proxy))
        .collect();
    assert_eq!(cards, vec![("aaaaaaaaaaaa", false), ("bbbbbbbbbbbb", true)]);
}

#[test]
fn card_hashes_skips_a_marker_that_is_not_twelve_hex_characters() {
    assert!(card_hashes(b"[keeper:12345] [keeper:zzzzzzzzzzzz]").is_empty());
    assert_eq!(
        card_hashes(b"x[keeper:0123456789ab]y"),
        vec!["0123456789ab"]
    );
}

#[test]
fn ranker_scores_reach_the_supply_record_of_the_session_that_was_ranked() {
    let snapshot = LessonSnapshot::from_pairs([(
        "/repo".into(),
        vec![("correction".into(), "cache invalidation rule".into())],
    )]);
    let candidates: Vec<_> = snapshot.for_project("/repo").iter().collect();
    let session = "supplied-tests-session";
    let chosen = snapshot.relevance.select(&candidates, session, "cache", 1);
    let Some(lesson) = chosen.first() else {
        panic!("the ranker chose nothing");
    };
    let score = chosen_score(session, lesson.id);
    assert!(score.is_some_and(|s| s > 0.0));
    assert_eq!(chosen_score("another-session", lesson.id), None);
    remember_scores(session, &[]);
    let evidence = read_evidence(session, b"", b"", &[lesson.id]);
    assert_eq!(evidence.lessons[0].score, score);
}
