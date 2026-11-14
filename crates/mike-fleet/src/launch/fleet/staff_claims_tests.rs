use std::collections::BTreeSet;
use std::time::Instant;

use uuid::Uuid;

use super::{CLAIM_PATIENCE, end_session, held_by, settle};
use crate::launch::fleet::plan::FleetFacts;
use crate::launch::fleet::staff::claim_all;
use crate::launch::fleet::test_fixtures::{complete, facts, parsed};

fn facts_for_new_run() -> FleetFacts {
    let mut facts = facts();
    facts.run_id = format!("fleet-claims-{}", Uuid::new_v4());
    facts
}

fn queued(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn claim(
    def: &crate::launch::fleet::definition::FleetDefinition,
    facts: &FleetFacts,
    task: &str,
) -> Vec<(String, Uuid)> {
    claim_all((def, facts), [task.to_string()].into_iter())
}

#[test]
fn a_task_stays_claimed_after_its_launch_row_opens_until_status_shows_the_seat() {
    let def = parsed(&complete());
    let facts = facts_for_new_run();
    let first = claim(&def, &facts, "P1");
    assert_eq!(first.len(), 1);

    assert!(claim(&def, &facts, "P1").is_empty());

    settle(&facts.run_id, &queued(&["P1", "P2"]), Instant::now());
    assert_eq!(held_by(&facts.run_id), queued(&["P1"]));
    assert!(
        claim(&def, &facts, "P1").is_empty(),
        "a tick between the launch row opening and status confirming the seat must not claim again"
    );

    settle(&facts.run_id, &queued(&["P2"]), Instant::now());
    assert!(held_by(&facts.run_id).is_empty());
}

#[test]
fn a_launch_that_failed_releases_only_its_own_claim() {
    let def = parsed(&complete());
    let facts = facts_for_new_run();
    let first = claim(&def, &facts, "P1");
    let session = first[0].1;

    end_session(&facts.run_id, "P1", Uuid::new_v4());
    assert!(claim(&def, &facts, "P1").is_empty());

    end_session(&facts.run_id, "P1", session);
    assert_eq!(claim(&def, &facts, "P1").len(), 1);
}

#[test]
fn a_claim_never_confirmed_is_released_after_the_patience() {
    let def = parsed(&complete());
    let facts = facts_for_new_run();
    claim(&def, &facts, "P1");

    settle(&facts.run_id, &queued(&["P1"]), Instant::now());
    assert_eq!(held_by(&facts.run_id).len(), 1);

    settle(
        &facts.run_id,
        &queued(&["P1"]),
        Instant::now() + CLAIM_PATIENCE,
    );
    assert!(held_by(&facts.run_id).is_empty());
}

#[test]
fn settling_one_run_leaves_another_runs_claims_alone() {
    let def = parsed(&complete());
    let (mine, other) = (facts_for_new_run(), facts_for_new_run());
    claim(&def, &mine, "P1");
    claim(&def, &other, "P1");

    settle(&mine.run_id, &queued(&[]), Instant::now());

    assert!(held_by(&mine.run_id).is_empty());
    assert_eq!(held_by(&other.run_id), queued(&["P1"]));
}
