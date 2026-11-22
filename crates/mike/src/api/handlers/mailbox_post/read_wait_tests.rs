use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use super::{Mark, Pending, read_key};

const WAIT: Duration = Duration::from_millis(300);

async fn returns_at_once(pending: &Pending, key: Uuid) -> bool {
    let started = tokio::time::Instant::now();
    pending.settled(key, WAIT).await;
    started.elapsed() < Duration::from_millis(100)
}

#[test]
fn read_key_names_only_mailbox_reads() {
    let session = Uuid::new_v4();
    assert_eq!(
        read_key(&format!("/api/sessions/{session}/messages")),
        Some(session)
    );
    assert_eq!(
        read_key(&format!(
            "/api/sessions/{session}/messages/{}",
            Uuid::new_v4()
        )),
        Some(session)
    );
    assert_eq!(
        read_key(&format!("/api/sessions/{session}/threads")),
        Some(session)
    );
    assert_eq!(read_key(&format!("/api/threads/{session}")), Some(session));
    assert_eq!(read_key("/api/health"), None);
    assert_eq!(read_key("/api/metrics"), None);
    assert_eq!(read_key("/api/threads"), None);
    assert_eq!(read_key(&format!("/api/sessions/{session}/overview")), None);
    assert_eq!(
        read_key(&format!("/api/sessions/{session}/mail/claim")),
        None
    );
    assert_eq!(read_key("/api/sessions/not-a-uuid/messages"), None);
}

#[tokio::test]
async fn pending_send_holds_back_reads_of_its_sessions_only() {
    let pending = Arc::new(Pending::default());
    let (to, from, other) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let mark = Mark::new(pending.clone(), vec![to, from]);
    assert!(returns_at_once(&pending, other).await);
    assert!(!returns_at_once(&pending, to).await);
    assert!(!returns_at_once(&pending, from).await);
    mark.release();
    assert!(returns_at_once(&pending, to).await);
    assert!(returns_at_once(&pending, from).await);
}

#[tokio::test]
async fn pending_send_released_twice_or_dropped_counts_once() {
    let pending = Arc::new(Pending::default());
    let to = Uuid::new_v4();
    let first = Mark::new(pending.clone(), vec![to]);
    let second = Mark::new(pending.clone(), vec![to]);
    first.release();
    first.release();
    assert!(
        !returns_at_once(&pending, to).await,
        "the second send is still pending"
    );
    drop(second);
    assert!(returns_at_once(&pending, to).await);
}

#[tokio::test]
async fn pending_send_released_wakes_a_waiting_read() {
    let pending = Arc::new(Pending::default());
    let to = Uuid::new_v4();
    let mark = Mark::new(pending.clone(), vec![to]);
    let waiting = {
        let pending = pending.clone();
        tokio::spawn(async move { pending.settled(to, Duration::from_secs(5)).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    mark.release();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .is_ok()
    );
}
