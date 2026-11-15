use super::super::ledger::LedgerEntry;
use super::*;

fn day(year: i32, month: u32, date: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, date).unwrap_or_default()
}

fn ledger(entries: &[(&str, &str)]) -> Ledger {
    entries
        .iter()
        .map(|(key, first)| {
            (
                (*key).to_owned(),
                LedgerEntry {
                    first_emitted: (*first).to_owned(),
                    ..LedgerEntry::default()
                },
            )
        })
        .collect()
}

fn set(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|key| (*key).to_owned()).collect()
}

fn run(
    ledger: &Ledger,
    loads: &[(&str, i64)],
    live: &BTreeSet<String>,
    fresh: &BTreeSet<String>,
    reviewed: &BTreeSet<String>,
    cap: usize,
) -> (Vec<String>, Vec<String>) {
    let loads: BTreeMap<String, i64> = loads.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
    let lessons = BTreeMap::new();
    enforce_cap(
        &CapInputs {
            ledger,
            loads: &loads,
            today: day(2026, 10, 14),
            live,
            fresh,
            lessons: &lessons,
            reviewed,
        },
        cap,
    )
}

#[test]
fn reworded_topics_share_a_purpose_and_a_single_shared_word_does_not() {
    assert!(same_purpose("deploying-the-proxy", "proxy-deploy-steps"));
    assert!(same_purpose("database-migrations", "database-migration"));
    assert!(!same_purpose("database-migrations", "migration-rollback"));
    assert!(!same_purpose("testing", "testing-flaky-db-tests"));
    assert!(!same_purpose("", "the-of"));
}

#[test]
fn the_cap_defaults_to_twelve_and_ignores_a_bad_setting() {
    assert_eq!(DEFAULT_CAP, 12);
    assert_eq!(cap_from_setting(None), 12);
    assert_eq!(cap_from_setting(Some(&serde_json::json!(5))), 5);
    for bad in [
        serde_json::json!(0),
        serde_json::json!(101),
        serde_json::json!("many"),
        serde_json::json!(true),
        serde_json::json!(null),
        serde_json::json!(5.5),
    ] {
        assert_eq!(cap_from_setting(Some(&bad)), 12, "{bad}");
    }
}

#[test]
fn the_cap_does_not_touch_a_project_within_it() {
    let ledger = ledger(&[("p:a", "2026-09-01"), ("p:b", "2026-09-01")]);
    let found = run(&ledger, &[], &set(&["p:a", "p:b"]), &set(&[]), &set(&[]), 2);
    assert_eq!(found, (Vec::<String>::new(), Vec::<String>::new()));
}

#[test]
fn ties_on_loads_retire_the_oldest_and_projects_are_counted_separately() {
    let ledger = ledger(&[
        ("p:a", "2026-09-02"),
        ("p:b", "2026-09-01"),
        ("p:c", "2026-10-14"),
        ("q:a", "2026-09-01"),
    ]);
    let live = set(&["p:a", "p:b", "p:c", "q:a"]);
    let (retire, hold) = run(&ledger, &[], &live, &set(&["p:c"]), &set(&[]), 2);
    assert_eq!((retire, hold), (vec!["p:b".to_owned()], Vec::new()));
}

#[test]
fn a_reviewed_skill_displaces_a_generated_one_even_a_young_one() {
    let ledger = ledger(&[
        ("p:gen", "2026-10-13"),
        ("p:old", "2026-09-01"),
        ("p:new", "2026-10-14"),
    ]);
    let live = set(&["p:gen", "p:old", "p:new"]);
    let (retire, hold) = run(
        &ledger,
        &[("p:gen", 5)],
        &live,
        &set(&["p:new"]),
        &set(&["p:old", "p:new"]),
        2,
    );
    assert_eq!((retire, hold), (vec!["p:gen".to_owned()], Vec::new()));
    let (retire, hold) = run(
        &ledger,
        &[],
        &live,
        &set(&["p:new", "p:gen"]),
        &set(&["p:new"]),
        2,
    );
    assert_eq!((retire, hold), (vec!["p:old".to_owned()], Vec::new()));
}

#[test]
fn overlapping_candidates_keep_the_shipped_one_then_the_one_with_more_lessons() {
    let candidates: Candidates = BTreeMap::from([
        (
            "p:deploying-the-proxy".to_owned(),
            ("deploying-the-proxy".to_owned(), 2),
        ),
        (
            "p:proxy-deploy-steps".to_owned(),
            ("proxy-deploy-steps".to_owned(), 3),
        ),
        ("p:lint".to_owned(), ("lint".to_owned(), 2)),
    ]);
    let held = hold_overlapping(&candidates, &set(&[]));
    assert_eq!(
        held,
        BTreeMap::from([(
            "p:deploying-the-proxy".to_owned(),
            "p:proxy-deploy-steps".to_owned()
        )])
    );
    let shipped = hold_overlapping(&candidates, &set(&["p:deploying-the-proxy"]));
    assert_eq!(
        shipped,
        BTreeMap::from([(
            "p:proxy-deploy-steps".to_owned(),
            "p:deploying-the-proxy".to_owned()
        )])
    );
}
