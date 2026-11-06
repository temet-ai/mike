use super::wait_backing::{BackingRef, WaitBacking, decide};

fn r(source: &str) -> BackingRef {
    BackingRef {
        source: source.to_owned(),
        ..BackingRef::default()
    }
}

#[test]
fn each_backing_kind_names_itself() {
    for (source, expected) in [
        ("work_item", WaitBacking::WorkItem),
        ("scheduled_wake", WaitBacking::ScheduledWake),
        ("monitor", WaitBacking::Monitor),
        ("peer_reply", WaitBacking::PeerReply),
        ("operator_requested", WaitBacking::OperatorRequested),
    ] {
        assert_eq!(decide(vec![r(source)]).0, expected, "{source}");
    }
}

#[test]
fn the_strongest_backing_wins_and_every_ref_is_kept() {
    let (value, refs) = decide(vec![
        r("peer_reply"),
        r("work_item"),
        r("operator_requested"),
    ]);
    assert_eq!(value, WaitBacking::WorkItem);
    assert_eq!(refs.len(), 3);
}

#[test]
fn monitor_schedule_wakeup_and_cron_launches_read_unknown() {
    let (value, _) = decide(vec![r("unregistered_launch")]);
    assert_eq!(value, WaitBacking::Unknown);
    let (backed, _) = decide(vec![r("unregistered_launch"), r("peer_reply")]);
    assert_eq!(backed, WaitBacking::PeerReply);
}

#[test]
fn nothing_found_is_none() {
    assert_eq!(decide(Vec::new()).0, WaitBacking::None);
}

#[test]
fn backing_refs_are_canonical_regardless_of_query_order() {
    let mut peer = r("peer_reply");
    peer.id = Some(2);
    let mut work = r("work_item");
    work.id = Some(1);
    let (_, forward) = decide(vec![peer.clone(), work.clone()]);
    let (_, reverse) = decide(vec![work, peer]);
    assert_eq!(forward, reverse);
}
