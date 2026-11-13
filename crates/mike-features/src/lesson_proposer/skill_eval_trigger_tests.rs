use super::tests::{material, results};
use super::trigger::{Choice, batch_count, batch_indexes};
use super::*;

fn choice(message: i64, load: &[&str]) -> Choice {
    Choice {
        message,
        load: load.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[test]
fn listing_rotates_so_no_skill_always_comes_first() {
    let entries = vec![
        ("a".to_owned(), "first".to_owned()),
        ("b".to_owned(), "second".to_owned()),
        ("c".to_owned(), "third".to_owned()),
    ];
    assert_eq!(
        trigger::listing(&entries, 0),
        "- a: first\n- b: second\n- c: third\n"
    );
    assert_eq!(
        trigger::listing(&entries, 1),
        "- b: second\n- c: third\n- a: first\n"
    );
    assert_eq!(trigger::listing(&entries, 4), trigger::listing(&entries, 1));
}

#[test]
fn triggered_matches_the_skill_name_exactly() {
    let loaded = vec![" database-migrations ".to_owned(), "other".to_owned()];
    assert!(trigger::triggered(&loaded, "database-migrations"));
    assert!(!trigger::triggered(&loaded, "database"));
    assert!(!trigger::triggered(&[], "database-migrations"));
}

#[test]
fn trigger_score_is_the_share_of_queries_answered_as_labelled() {
    let scored = trigger::score(results(3, 4));
    assert_eq!((scored.correct, scored.total), (3, 4));
    assert!((scored.score - 0.75).abs() < 1e-9);
    assert!(trigger::score(Vec::new()).score.abs() < 1e-9);
}

#[test]
fn twenty_queries_are_two_batches_and_every_query_is_asked_exactly_once() {
    assert_eq!(batch_count(20), 2);
    assert_eq!(batch_count(21), 3);
    assert_eq!(batch_count(1), 1);
    assert_eq!(batch_count(0), 0);
    for total in [1, 9, 10, 11, 20, 21, 30] {
        let queries = material(total).queries;
        let mut seen: Vec<usize> = (0..batch_count(total))
            .flat_map(|batch| batch_indexes(&queries, batch))
            .collect();
        assert!(
            (0..batch_count(total))
                .all(|b| batch_indexes(&queries, b).len() <= trigger::BATCH_SIZE)
        );
        seen.sort_unstable();
        assert_eq!(seen, (0..total).collect::<Vec<_>>(), "{total} queries");
    }
}

#[test]
fn every_batch_gets_half_of_the_queries_that_should_trigger_whatever_order_they_came_in() {
    let mut queries = material(20).queries;
    queries.sort_by_key(|q| q.should_trigger);
    for batch in 0..2 {
        let picked = batch_indexes(&queries, batch);
        let wanted = picked
            .iter()
            .filter_map(|&i| queries.get(i))
            .filter(|q| q.should_trigger)
            .count();
        assert_eq!((picked.len(), wanted), (10, 5), "batch {batch}");
        assert!(picked.windows(2).all(|w| w[0] < w[1]));
    }
}

#[test]
fn batch_input_lists_the_skills_then_numbers_the_messages() {
    let entries = vec![("a".to_owned(), "first".to_owned())];
    let input = trigger::batch_input(&entries, 0, &[" one ", "two"]);
    assert_eq!(
        input,
        "## AVAILABLE SKILLS\n- a: first\n\n## USER MESSAGES\n1. one\n\n2. two\n\n"
    );
}

#[test]
fn the_trigger_test_names_the_project_before_the_skills_when_it_has_one() {
    let entries = vec![("a".to_owned(), "first".to_owned())];
    let with_project = trigger::Offer {
        name: "a",
        entries: &entries,
        project: "Belongs to /work/mike. Top level: docs/\n",
    };
    let input = trigger::offer_input(&with_project, 0, &["one"]);
    assert!(input.starts_with(
        "## THE PROJECT\nBelongs to /work/mike. Top level: docs/\n\n## AVAILABLE SKILLS\n- a: first\n"
    ));
    let without = trigger::Offer {
        project: "  ",
        ..with_project
    };
    assert!(trigger::offer_input(&without, 0, &["one"]).starts_with("## AVAILABLE SKILLS\n"));
}

#[test]
fn answers_are_matched_to_messages_by_number_in_any_order() -> Result<(), EvalError> {
    let m = material(4);
    let queries: Vec<&TriggerQuery> = m.queries.iter().take(3).collect();
    let answers = vec![
        choice(3, &["database-migrations"]),
        choice(1, &["database-migrations"]),
        choice(2, &["other"]),
    ];
    let matched = trigger::matched(&queries, answers, "database-migrations")?;
    assert_eq!(matched[0].query, "query 0");
    assert!(matched[0].correct && matched[0].triggered);
    assert!(matched[1].correct && !matched[1].triggered);
    assert_eq!(matched[1].loaded, vec!["other"]);
    assert!(matched[2].correct);
    Ok(())
}

#[test]
fn answers_with_a_missing_duplicate_or_extra_message_are_an_error() {
    let m = material(4);
    let queries: Vec<&TriggerQuery> = m.queries.iter().take(2).collect();
    for answers in [
        vec![choice(1, &[])],
        vec![choice(1, &[]), choice(1, &[])],
        vec![choice(1, &[]), choice(3, &[])],
        vec![choice(1, &[]), choice(2, &[]), choice(3, &[])],
    ] {
        match trigger::matched(&queries, answers, "database-migrations") {
            Err(EvalError::Model(message)) => assert!(message.contains("trigger test answered")),
            other => panic!("expected a trigger test error, got {other:?}"),
        }
    }
}

#[test]
fn the_draft_is_always_listed_first_and_competitors_are_not_repeated() {
    let sibling = |folder: &str| SiblingSkill {
        folder: folder.to_owned(),
        description: format!("about {folder}"),
    };
    let entries = trigger::entries(
        "mine",
        "my description",
        &[sibling("ui"), sibling("mine")],
        &[sibling("ui"), sibling("lint")],
    );
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["mine", "ui", "lint"]);
    assert_eq!(entries[0].1, "my description");
}
