use std::path::PathBuf;

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::context::{Here, Place};
use super::fakes_tests::{BUSY_PROBE, QUIET_PROBE, remote_fixer, spawned};
use super::file::parse_created;
use super::handoff::parse_spawned;
use super::quiet::parse_probe;
use super::runner::sh_quote;
use super::sessions::silent_for;
use crate::store::persistence::wright::Fixer;

fn here(checkout: Option<&str>) -> Here {
    Here {
        host: "omit".to_owned(),
        checkout: checkout.map(PathBuf::from),
        loadavg: PathBuf::from("/proc/loadavg"),
    }
}

#[test]
fn probe_quiet_load_and_free_lock_is_quiet() {
    assert_eq!(parse_probe(QUIET_PROBE), Ok(()));
}

#[test]
fn probe_load_over_processors_is_busy_and_a_running_build_is_not() {
    assert!(parse_probe(BUSY_PROBE).is_err_and(|why| why.contains("12.50")));
    assert_eq!(parse_probe("0.1 0.1 0.1 1/1 1\n8\n"), Ok(()));
    assert!(parse_probe("0.1 0.1\n").is_err());
}

#[test]
fn spawned_json_with_opened_session_gives_its_uuid() {
    let session = Uuid::new_v4();
    let out = format!("motd line\n{}", spawned(session));
    assert_eq!(parse_spawned(&out), Ok(session));
    let refused = r#"{"session":null,"opened":false,"reason":"surface cap reached"}"#;
    assert!(parse_spawned(refused).is_err_and(|why| why.contains("surface cap")));
}

#[test]
fn created_issue_url_gives_number_and_url() {
    let out = "Creating issue in temet-ai/mike\n\nhttps://github.com/temet-ai/mike/issues/312\n";
    assert_eq!(
        parse_created(out),
        Some((
            312,
            "https://github.com/temet-ai/mike/issues/312".to_owned()
        ))
    );
    assert_eq!(parse_created("no url"), None);
}

#[test]
fn sh_quote_keeps_quotes_and_metacharacters_literal() {
    assert_eq!(sh_quote("it's $(rm -rf /)"), r"'it'\''s $(rm -rf /)'");
}

#[test]
fn silent_fixer_ends_only_after_six_hours() {
    let now = Utc::now();
    let session = Uuid::new_v4();
    assert!(silent_for(now - Duration::hours(5), now, session).is_none());
    assert!(silent_for(now - Duration::hours(6), now, session).is_some());
}

#[test]
fn targets_empty_list_is_the_local_checkout_or_nothing() {
    let local = here(Some("/srv/mike")).targets(&[]);
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].place, Place::Local);
    assert_eq!(local[0].folder, "/srv/mike");
    assert!(here(None).targets(&[]).is_empty());
    assert!(here(None).unreachable(&[]).is_some());
}

#[test]
fn targets_keep_remote_hosts_and_this_machine_only() {
    let other = Fixer {
        machine: "mac".to_owned(),
        host: None,
        folder: Some("/Users/x/mike".to_owned()),
    };
    let this = Fixer {
        machine: "omit".to_owned(),
        host: None,
        folder: None,
    };
    let targets = here(Some("/srv/mike")).targets(&[remote_fixer(), other, this]);
    let places: Vec<_> = targets.iter().map(|t| t.place.clone()).collect();
    assert_eq!(
        places,
        vec![Place::Remote("infra".to_owned()), Place::Local]
    );
    assert_eq!(targets[1].folder, "/srv/mike");
}
