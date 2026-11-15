use super::super::test_support::lesson;
use super::*;

fn memory(id: i64) -> LessonRow {
    LessonRow {
        placement: Some("memory".to_owned()),
        ..lesson(id, None)
    }
}

fn route_of(decisions: &[RouteDecision], id: i64) -> Option<LessonRoute> {
    decisions
        .iter()
        .find(|decision| decision.lesson_id == id)
        .map(|decision| decision.route)
}

#[test]
fn a_lesson_a_hand_written_file_already_says_is_covered_and_names_the_file() {
    let lessons = vec![memory(1), memory(2)];
    let covered = BTreeMap::from([(1, "CLAUDE.md".to_owned())]);
    let decisions = decide_routes(&lessons, &covered);
    assert_eq!(route_of(&decisions, 1), Some(LessonRoute::Covered));
    assert_eq!(decisions[0].covered_by.as_deref(), Some("CLAUDE.md"));
    assert_eq!(route_of(&decisions, 2), Some(LessonRoute::Standing));
    assert!(decisions[1].covered_by.is_none());
}

#[test]
fn standing_lessons_are_the_memory_placed_long_term_or_operator_ones() {
    let mut long_term_agent_less = memory(1);
    long_term_agent_less.source = "judge".to_owned();
    let mut raw_judge = memory(2);
    raw_judge.source = "judge".to_owned();
    raw_judge.stage = "raw".to_owned();
    let mut raw_operator = memory(3);
    raw_operator.stage = "raw".to_owned();
    let unplaced = LessonRow {
        placement: None,
        ..lesson(4, None)
    };
    let decisions = decide_routes(
        &[long_term_agent_less, raw_judge, raw_operator, unplaced],
        &BTreeMap::new(),
    );
    assert_eq!(route_of(&decisions, 1), Some(LessonRoute::Standing));
    assert_eq!(route_of(&decisions, 2), Some(LessonRoute::Injection));
    assert_eq!(route_of(&decisions, 3), Some(LessonRoute::Standing));
    assert_eq!(route_of(&decisions, 4), Some(LessonRoute::Standing));
}

#[test]
fn lessons_that_feed_skills_or_belong_to_an_agent_or_a_shared_workflow_stay_injected() {
    let skill = LessonRow {
        placement: Some("skill".to_owned()),
        ..lesson(1, None)
    };
    let unplaced_with_topic = LessonRow {
        placement: None,
        ..lesson(2, Some("deploy"))
    };
    let agent = LessonRow {
        agent_id: Some(7),
        ..memory(3)
    };
    let shared = LessonRow {
        topic: Some("shared:release".to_owned()),
        ..memory(4)
    };
    let verifier = LessonRow {
        topic: Some("verifier:met".to_owned()),
        ..memory(5)
    };
    let rule = LessonRow {
        placement: Some("rule".to_owned()),
        ..lesson(6, None)
    };
    let decisions = decide_routes(
        &[skill, unplaced_with_topic, agent, shared, verifier, rule],
        &BTreeMap::new(),
    );
    assert!(decisions.iter().all(|d| d.route == LessonRoute::Injection));
}

#[test]
fn at_most_sixty_lessons_stand_and_the_newest_win() {
    let lessons: Vec<LessonRow> = (1..=70).map(memory).collect();
    let decisions = decide_routes(&lessons, &BTreeMap::new());
    let standing: Vec<i64> = decisions
        .iter()
        .filter(|d| d.route == LessonRoute::Standing)
        .map(|d| d.lesson_id)
        .collect();
    assert_eq!(standing.len(), STANDING_CAP);
    assert_eq!(standing.first(), Some(&11));
    assert_eq!(standing.last(), Some(&70));
    assert_eq!(route_of(&decisions, 10), Some(LessonRoute::Injection));
}

#[test]
fn a_covered_lesson_does_not_take_a_standing_place() {
    let lessons: Vec<LessonRow> = (1..=61).map(memory).collect();
    let covered = BTreeMap::from([(61, "AGENTS.md".to_owned())]);
    let decisions = decide_routes(&lessons, &covered);
    assert_eq!(route_of(&decisions, 61), Some(LessonRoute::Covered));
    assert_eq!(route_of(&decisions, 1), Some(LessonRoute::Standing));
}

#[test]
fn the_decision_carries_the_lesson_revision_and_project() {
    let mut row = memory(5);
    row.revision = 4;
    row.project_id = 9;
    let decisions = decide_routes(&[row], &BTreeMap::new());
    assert_eq!((decisions[0].revision, decisions[0].project_id), (4, 9));
}

#[test]
fn standing_rows_come_newest_first() {
    let lessons = vec![memory(1), memory(2), memory(3)];
    let decisions = decide_routes(&lessons, &BTreeMap::from([(2, "CLAUDE.md".to_owned())]));
    let rows = standing_rows(&lessons, &decisions);
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![3, 1]
    );
}
