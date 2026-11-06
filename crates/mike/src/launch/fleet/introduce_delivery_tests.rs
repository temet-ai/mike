use std::cell::Cell;

use super::tests::measured_repeats;
use super::*;

#[test]
fn a_second_seat_at_the_same_state_is_the_only_one_noticed() {
    let slow = vec![command(1, 0, false), command(2, 10, false)];
    let repeats = measured_repeats(&slow, &[]);
    assert_eq!(repeats.len(), 1);
    assert_eq!(repeats[0].first.seat, "dev-1");
    assert_eq!(repeats[0].again.seat, "dev-2");
    assert_eq!(repeats[0].again.session, slow[1].session);
}

#[tokio::test]
async fn only_a_queued_notice_keeps_the_introduction_claim() {
    let outcomes: [(Result<Option<()>, &str>, usize); 3] = [
        (Ok(Some(())), 0),
        (Ok(None), 1),
        (Err("mailbox unavailable"), 1),
    ];
    for (sent, expected_releases) in outcomes {
        let releases = Cell::new(0);
        release_unqueued("run", sent, async {
            releases.set(releases.get() + 1);
            Ok(())
        })
        .await;
        assert_eq!(releases.get(), expected_releases);
    }
}

#[tokio::test]
async fn a_failed_claim_release_is_attempted_once() {
    let releases = Cell::new(0);
    release_unqueued("run", Ok::<Option<()>, &str>(None), async {
        releases.set(releases.get() + 1);
        Err("claim storage unavailable")
    })
    .await;
    assert_eq!(releases.get(), 1);
}

fn command(use_id: i64, minutes: i64, background: bool) -> SlowCommandRow {
    let started_at =
        DateTime::from_timestamp(1_700_000_000, 0).unwrap_or_default() + Duration::minutes(minutes);
    SlowCommandRow {
        use_id,
        seat: format!("dev-{use_id}"),
        session: Uuid::from_u128(u128::try_from(use_id).unwrap_or_default()),
        started_at,
        finished_at: started_at + Duration::seconds(30),
        is_error: false,
        in_background: background,
        command: "make test".into(),
    }
}

#[test]
fn a_background_launch_is_not_a_completed_first_result() {
    let first = command(1, 0, true);
    let again = command(2, 5, false);
    assert!(measured_repeats(&[first.clone(), again.clone()], &[]).is_empty());
    let repeat = Repeat {
        work: "make test".into(),
        first,
        again,
        first_seconds: 30.0,
        again_seconds: 30.0,
    };
    assert!(!no_observed_invalidation(&[], &repeat));
    let body = mail(&repeat, &[]);
    assert!(body.contains("background launch; completion unknown"));
    assert!(!body.contains("first result already held"));
    assert!(!body.contains("and it passed"));
}

#[test]
fn an_intervening_background_launch_does_not_hide_a_completed_first_result() {
    let found = measured_repeats(
        &[
            command(1, 0, false),
            command(2, 5, true),
            command(3, 10, false),
        ],
        &[],
    );
    let pairs: Vec<_> = found
        .iter()
        .map(|repeat| (repeat.again.use_id, repeat.first.use_id))
        .collect();
    assert_eq!(pairs, [(3, 1)]);
    assert!(
        found
            .iter()
            .all(|repeat| no_observed_invalidation(&[], repeat))
    );
    assert!(!mail(&found[0], &[]).contains("started in the background"));
    assert!(!mail(&found[0], &[]).contains("hung"));
}
