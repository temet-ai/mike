use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::{Clock, earliest_flip, in_scope, roster_row};
use crate::store::persistence::runtime::SessionObservationRow;
use crate::store::persistence::workspace::roster::members::{RosterChannel, RosterMember};
use crate::store::persistence::workspace::roster::state::{ChannelState, MemberState};
use crate::workspace::{Liveness, RosterScope};

fn member(registered_at: DateTime<Utc>) -> RosterMember {
    RosterMember {
        session_id: 7,
        session_uuid: Uuid::from_u128(7),
        cwd: "/repo".to_owned(),
        worktree: None,
        registered_via: "hook".to_owned(),
        registered_at,
        released_at: None,
        channels: vec![RosterChannel {
            id: 70,
            cse_id: "cse_seven".to_owned(),
            client_version: Some("2.1.300".to_owned()),
        }],
    }
}

fn observation(observed_at: Option<DateTime<Utc>>) -> SessionObservationRow {
    SessionObservationRow {
        session_uuid: Uuid::from_u128(7),
        runtime: "claude".to_owned(),
        source: "wire".to_owned(),
        source_version: None,
        fidelity: "wire".to_owned(),
        completeness: "captured".to_owned(),
        observed_at,
        lifecycle: None,
        lifecycle_at: None,
        control: None,
        requested_model: None,
        observed_model: None,
        usage_scope: None,
        usage_basis: None,
        input_tokens: None,
        cached_input_tokens: None,
        output_tokens: None,
        reasoning_output_tokens: None,
        total_tokens: None,
        context_window: None,
        model_call_count: 0,
    }
}

fn state(lease_expires_at: DateTime<Utc>, now: DateTime<Utc>) -> MemberState {
    MemberState {
        lease_expires_at,
        goal: None,
        next_steps: None,
        display_name: None,
        role: None,
        instructions: None,
        newest_request_number: None,
        machine_uuid: None,
        machine_name: None,
        observation: observation(Some(now - Duration::minutes(1))),
        channel: None,
    }
}

fn heartbeat(id: i64, at: DateTime<Utc>) -> ChannelState {
    ChannelState {
        id,
        last_heartbeat_at: Some(at),
        last_heartbeat_interval_seconds: Some(20),
        last_idle_seconds: Some(0),
        worker_status: Some("idle".to_owned()),
        worker_status_at: Some(at),
        stream_open: false,
        stream_opened_at: None,
        last_delivery_at: None,
        post_turn_summary: None,
        post_turn_summary_at: None,
    }
}

#[test]
fn a_released_member_reads_ended_and_only_the_all_scope_keeps_it() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let mut released = member(now - Duration::minutes(1));
    released.released_at = Some(now);
    let live_state = state(now + Duration::minutes(5), now);
    assert_eq!(
        roster_row(&released, &live_state, &clock).liveness,
        Liveness::Ended
    );
    assert!(!in_scope(&released, &live_state, RosterScope::Live, &clock));
    assert!(!in_scope(&released, &live_state, RosterScope::Active, &clock));
    assert!(in_scope(&released, &live_state, RosterScope::All, &clock));
}

#[test]
fn a_member_never_captured_after_its_grace_reads_uncaptured_and_leaves_the_live_scope() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let quiet = member(now - Duration::minutes(11));
    let mut silent = state(now + Duration::minutes(5), now);
    silent.observation.observed_at = None;
    let row = roster_row(&quiet, &silent, &clock);
    assert_eq!(row.liveness, Liveness::Uncaptured);
    assert!(!in_scope(&quiet, &silent, RosterScope::Live, &clock));
    assert!(in_scope(&quiet, &silent, RosterScope::Active, &clock));
}

#[test]
fn a_lapsed_lease_with_recent_activity_reads_live_from_activity() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let lapsed = state(now - Duration::minutes(1), now);
    let row = roster_row(&member(now - Duration::hours(1)), &lapsed, &clock);
    assert_eq!(
        (row.liveness, row.liveness_source),
        (Liveness::Live, "activity")
    );
}

#[test]
fn a_lapsed_lease_without_recent_activity_reads_stale_and_only_active_keeps_it() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let old = member(now - Duration::hours(1));
    let mut idle = state(now - Duration::minutes(1), now);
    idle.observation.observed_at = Some(now - Duration::minutes(16));
    assert_eq!(roster_row(&old, &idle, &clock).liveness, Liveness::Stale);
    assert!(!in_scope(&old, &idle, RosterScope::Live, &clock));
    assert!(in_scope(&old, &idle, RosterScope::Active, &clock));
}

#[test]
fn an_ended_lifecycle_reads_ended_even_with_a_fresh_channel() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let mut ended = state(now + Duration::minutes(5), now);
    ended.observation.lifecycle = Some("ended".to_owned());
    ended.channel = Some(heartbeat(70, now));
    let row = roster_row(&member(now), &ended, &clock);
    assert_eq!(
        (row.liveness, row.liveness_source),
        (Liveness::Ended, "channel")
    );
}

#[test]
fn a_channel_heartbeat_decides_liveness_and_carries_the_remembered_identity() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let mut fresh = state(now + Duration::minutes(5), now);
    fresh.channel = Some(heartbeat(70, now - Duration::seconds(10)));
    let row = roster_row(&member(now), &fresh, &clock);
    assert_eq!(row.liveness, Liveness::Live);
    assert_eq!(row.channel_cse_id.as_deref(), Some("cse_seven"));
    assert_eq!(row.client_version.as_deref(), Some("2.1.300"));
    assert_eq!(row.heartbeat_age_seconds, Some(10));

    let mut overdue = state(now + Duration::minutes(5), now);
    overdue.channel = Some(heartbeat(70, now - Duration::seconds(61)));
    assert_eq!(
        roster_row(&member(now), &overdue, &clock).liveness,
        Liveness::Stale
    );
}

#[test]
fn a_channel_state_for_an_unknown_channel_id_is_left_out() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let mut stray = state(now + Duration::minutes(5), now);
    stray.channel = Some(heartbeat(71, now));
    let row = roster_row(&member(now), &stray, &clock);
    assert!(!row.channel_present);
    assert_eq!(row.liveness_source, "lease");
}

#[test]
fn earliest_flip_with_a_fresh_channel_is_its_heartbeat_deadline() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let mut fresh = state(now + Duration::minutes(5), now);
    fresh.channel = Some(heartbeat(70, now - Duration::seconds(10)));
    let row = roster_row(&member(now), &fresh, &clock);
    assert_eq!(earliest_flip(&row, &clock), Some(now + Duration::seconds(50)));
}

#[test]
fn earliest_flip_without_a_channel_is_the_nearest_of_lease_and_activity() {
    let now = Utc::now();
    let clock = Clock::at(now);
    let leased = state(now + Duration::minutes(20), now);
    let row = roster_row(&member(now - Duration::hours(1)), &leased, &clock);
    assert_eq!(
        earliest_flip(&row, &clock),
        Some(now - Duration::minutes(1) + Duration::minutes(15))
    );
}
