use super::*;

#[tokio::test]
async fn completion_waits_for_admission_before_draining_its_waiters() -> anyhow::Result<()> {
    let run = format!("flight-race-{}", Uuid::new_v4());
    let session = Uuid::new_v4();
    let gate = gates()
        .entry(run.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let held = gate.lock().await;
    let (_sender, receiver) = watch::channel(None);
    in_flight().insert(
        run.clone(),
        InFlight {
            id: 1,
            running: true,
            state: "state".into(),
            commit: "commit".into(),
            started_at: Utc::now(),
            done: receiver,
        },
    );
    wait_for_mail(&run, session);
    let finished_run = run.clone();
    let mut completion = tokio::spawn(async move { landed(&finished_run, 1).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut completion)
            .await
            .is_err()
    );
    assert!(in_flight().contains_key(&run));
    assert!(waiting().contains_key(&run));
    drop(held);
    let sessions = tokio::time::timeout(Duration::from_secs(1), completion).await??;
    assert_eq!(sessions, vec![session]);
    assert!(!in_flight().contains_key(&run));
    assert!(!waiting().contains_key(&run));
    Ok(())
}

#[tokio::test]
async fn a_superseded_receipt_that_lands_leaves_the_newer_flight_and_its_waiters() {
    let run = format!("flight-superseded-{}", Uuid::new_v4());
    let session = Uuid::new_v4();
    let (_sender, receiver) = watch::channel(None);
    in_flight().insert(
        run.clone(),
        InFlight {
            id: 2,
            running: false,
            state: "newer".into(),
            commit: "commit".into(),
            started_at: Utc::now(),
            done: receiver,
        },
    );
    wait_for_mail(&run, session);
    assert!(landed(&run, 1).await.is_empty());
    assert!(in_flight().get(&run).is_some_and(|flight| flight.id == 2));
    assert_eq!(landed(&run, 2).await, vec![session]);
    assert!(!in_flight().contains_key(&run));
}
