use chrono::{DateTime, Duration, Utc};

use super::members::{RosterChannel, RosterMember};
use super::state::{ChannelState, MemberState};
use crate::store::persistence::session_machine::MachineRef;
use crate::store::persistence::workspace::{HEARTBEAT_LEASE_MINUTES, WorkspaceRosterRow};
use crate::workspace::{Liveness, RosterScope};

const MIN_HEARTBEAT_INTERVAL_SECONDS: i32 = 20;
const HEARTBEAT_DEAD_MULTIPLIER: i64 = 3;
pub(super) const UNCAPTURED_GRACE_MINUTES: i64 = 10;

#[derive(Debug, Clone, Copy)]
pub(super) struct Clock {
    pub(super) now: DateTime<Utc>,
    pub(super) activity_cutoff: DateTime<Utc>,
    pub(super) uncaptured_cutoff: DateTime<Utc>,
}

impl Clock {
    pub(super) fn at(now: DateTime<Utc>) -> Self {
        Self {
            now,
            activity_cutoff: now - Duration::minutes(HEARTBEAT_LEASE_MINUTES),
            uncaptured_cutoff: now - Duration::minutes(UNCAPTURED_GRACE_MINUTES),
        }
    }
}

struct Activity<'a> {
    member: &'a RosterMember,
    state: &'a MemberState,
}

impl Activity<'_> {
    fn last_request_at(&self) -> Option<DateTime<Utc>> {
        self.state.observation.observed_at
    }

    fn lifecycle_over(&self) -> bool {
        matches!(
            self.state.observation.lifecycle.as_deref(),
            Some("ended" | "failed")
        )
    }

    fn uncaptured(&self, clock: &Clock) -> bool {
        self.last_request_at().is_none() && self.member.registered_at < clock.uncaptured_cutoff
    }

    fn lease_or_recent(&self, clock: &Clock) -> bool {
        self.state.lease_expires_at > clock.now
            || self
                .last_request_at()
                .is_some_and(|at| at > clock.activity_cutoff)
    }

    fn liveness(&self, clock: &Clock) -> Liveness {
        if self.member.released_at.is_some() || self.lifecycle_over() {
            Liveness::Ended
        } else if self.uncaptured(clock) {
            Liveness::Uncaptured
        } else if self.lease_or_recent(clock) {
            Liveness::Live
        } else {
            Liveness::Stale
        }
    }

    fn in_scope(&self, scope: RosterScope, clock: &Clock) -> bool {
        let (allow_stale, allow_ended) = scope.filter_bools();
        let open = self.member.released_at.is_none() && !self.lifecycle_over();
        let current = !self.lifecycle_over() && !self.uncaptured(clock) && self.lease_or_recent(clock);
        (allow_ended || open) && (allow_stale || current)
    }
}

pub(super) fn in_scope(
    member: &RosterMember,
    state: &MemberState,
    scope: RosterScope,
    clock: &Clock,
) -> bool {
    Activity { member, state }.in_scope(scope, clock)
}

pub(super) fn roster_row(
    member: &RosterMember,
    state: &MemberState,
    clock: &Clock,
) -> WorkspaceRosterRow {
    let liveness = Activity { member, state }.liveness(clock);
    let lease_lapsed = state.lease_expires_at <= clock.now;
    let mut row = WorkspaceRosterRow {
        session_uuid: member.session_uuid,
        cwd: member.cwd.clone(),
        worktree: member.worktree.clone(),
        registered_via: member.registered_via.clone(),
        registered_at: member.registered_at,
        lease_expires_at: state.lease_expires_at,
        released_at: member.released_at,
        goal: state.goal.clone(),
        next_steps: state.next_steps.clone(),
        display_name: state.display_name.clone(),
        role: state.role.clone(),
        instructions: state.instructions.clone(),
        liveness,
        last_request_at: state.observation.observed_at,
        newest_request_number: state.newest_request_number,
        machine: MachineRef::from_parts(state.machine_uuid, state.machine_name.clone()),
        channel_present: false,
        channel_cse_id: None,
        heartbeat_age_seconds: None,
        heartbeat_interval_seconds: None,
        idle_seconds: None,
        session_state: None,
        session_state_at: None,
        reply_channel_open: false,
        reply_channel_opened_at: None,
        last_inbound_message_at: None,
        post_turn_summary: None,
        post_turn_summary_at: None,
        client_version: None,
        liveness_source: if liveness == Liveness::Live && lease_lapsed {
            "activity"
        } else {
            "lease"
        },
        observation: Some(state.observation.clone()),
    };
    let channel = state.channel.as_ref().and_then(|channel| {
        member
            .channels
            .iter()
            .find(|identity| identity.id == channel.id)
            .map(|identity| (identity, channel))
    });
    if let Some((identity, channel)) = channel {
        merge_channel(&mut row, identity, channel, clock.now);
    }
    row
}

pub(super) fn dead_after_seconds(interval: Option<i32>) -> i64 {
    i64::from(
        interval
            .unwrap_or(MIN_HEARTBEAT_INTERVAL_SECONDS)
            .max(MIN_HEARTBEAT_INTERVAL_SECONDS),
    ) * HEARTBEAT_DEAD_MULTIPLIER
}

fn merge_channel(
    row: &mut WorkspaceRosterRow,
    identity: &RosterChannel,
    channel: &ChannelState,
    now: DateTime<Utc>,
) {
    row.channel_present = true;
    row.channel_cse_id = Some(identity.cse_id.clone());
    row.heartbeat_age_seconds = channel
        .last_heartbeat_at
        .map(|hb| (now - hb).num_seconds());
    row.heartbeat_interval_seconds = channel.last_heartbeat_interval_seconds;
    row.idle_seconds = channel.last_idle_seconds;
    row.session_state = channel.worker_status.clone();
    row.session_state_at = channel.worker_status_at;
    row.reply_channel_open = channel.stream_open;
    row.reply_channel_opened_at = channel
        .stream_open
        .then_some(channel.stream_opened_at)
        .flatten();
    row.last_inbound_message_at = channel.last_delivery_at;
    row.post_turn_summary = channel.post_turn_summary.clone();
    row.post_turn_summary_at = channel.post_turn_summary_at;
    row.client_version = identity.client_version.clone();
    row.liveness_source = "channel";

    if row.liveness == Liveness::Ended {
        return;
    }
    let threshold_seconds = dead_after_seconds(channel.last_heartbeat_interval_seconds);
    let channel_live = row
        .heartbeat_age_seconds
        .is_some_and(|age| age <= threshold_seconds);
    row.liveness = if channel_live {
        Liveness::Live
    } else {
        Liveness::Stale
    };
}

pub(super) fn earliest_flip(row: &WorkspaceRosterRow, clock: &Clock) -> Option<DateTime<Utc>> {
    let lease = Some(row.lease_expires_at);
    let activity = row
        .last_request_at
        .map(|at| at + Duration::minutes(HEARTBEAT_LEASE_MINUTES));
    let uncaptured = row
        .last_request_at
        .is_none()
        .then(|| row.registered_at + Duration::minutes(UNCAPTURED_GRACE_MINUTES));
    let heartbeat = row
        .channel_present
        .then_some(row.heartbeat_age_seconds)
        .flatten()
        .map(|age| {
            clock.now + Duration::seconds(dead_after_seconds(row.heartbeat_interval_seconds) - age)
        });
    [lease, activity, uncaptured, heartbeat]
        .into_iter()
        .flatten()
        .filter(|at| *at >= clock.now)
        .min()
}

#[cfg(test)]
#[path = "assemble_tests.rs"]
mod assemble_tests;
