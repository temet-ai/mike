use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{Decision, ReturnVia, choose_return, decide};
use crate::store::persistence::conditions::{Fired, SessionPresence};

fn presence(live: bool, idle_minutes: i64, cwd: Option<&str>) -> SessionPresence {
    SessionPresence {
        session_uuid: Uuid::new_v4(),
        project_id: 1,
        last_request_at: Utc::now() - Duration::minutes(idle_minutes),
        cwd: cwd.map(str::to_owned),
        live,
    }
}

#[test]
fn decide_fires_when_the_condition_holds_even_past_the_deadline() {
    let now = Utc::now();
    assert_eq!(decide(true, None, now), Decision::Fire(Fired::Held));
    let past = Some(now - Duration::minutes(5));
    assert_eq!(decide(true, past, now), Decision::Fire(Fired::Held));
}

#[test]
fn decide_marks_missed_when_the_deadline_passed_without_the_condition() {
    let now = Utc::now();
    let past = Some(now - Duration::seconds(1));
    assert_eq!(decide(false, past, now), Decision::Fire(Fired::Missed));
    assert_eq!(decide(false, Some(now), now), Decision::Fire(Fired::Missed));
}

#[test]
fn decide_waits_without_a_passed_deadline() {
    let now = Utc::now();
    assert_eq!(decide(false, None, now), Decision::Wait);
    let future = Some(now + Duration::minutes(1));
    assert_eq!(decide(false, future, now), Decision::Wait);
}

#[test]
fn choose_return_mails_a_live_session_whose_cache_is_warm() {
    let warm = presence(true, 30, None);
    assert_eq!(
        choose_return(Some(&warm), true, Utc::now()),
        ReturnVia::Mail
    );
}

#[test]
fn choose_return_live_but_cold_on_a_thread_starts_fresh_instead_of_rewriting_its_cache() {
    let cold = presence(true, 500, Some("/work/repo"));
    assert_eq!(
        choose_return(Some(&cold), true, Utc::now()),
        ReturnVia::Start
    );
}

#[test]
fn choose_return_live_but_cold_without_a_thread_still_mails() {
    let cold = presence(true, 500, None);
    assert_eq!(
        choose_return(Some(&cold), false, Utc::now()),
        ReturnVia::Mail
    );
}

#[test]
fn choose_return_resumes_a_session_closed_within_the_hour_with_a_known_folder() {
    let recent = presence(false, 59, Some("/work/repo"));
    assert_eq!(
        choose_return(Some(&recent), true, Utc::now()),
        ReturnVia::Resume
    );
}

#[test]
fn choose_return_starts_a_new_session_when_old_or_without_a_folder_or_unknown() {
    let old = presence(false, 61, Some("/work/repo"));
    assert_eq!(
        choose_return(Some(&old), true, Utc::now()),
        ReturnVia::Start
    );
    let no_folder = presence(false, 5, None);
    assert_eq!(
        choose_return(Some(&no_folder), true, Utc::now()),
        ReturnVia::Start
    );
    assert_eq!(choose_return(None, true, Utc::now()), ReturnVia::Start);
}
