use super::*;

#[test]
fn actor_label_uses_bare_uuid_when_no_title() {
    let uuid = uuid::Uuid::nil();
    assert_eq!(actor_label(uuid, None), uuid.to_string());
    assert_eq!(actor_label(uuid, Some("")), uuid.to_string());
    assert_eq!(actor_label(uuid, Some("   ")), uuid.to_string());
}

#[test]
fn actor_label_appends_title_when_present() {
    let uuid = uuid::Uuid::nil();
    assert_eq!(
        actor_label(uuid, Some("fixing the flaky test")),
        format!("{uuid} (fixing the flaky test)")
    );
}
