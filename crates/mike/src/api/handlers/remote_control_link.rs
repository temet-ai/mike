use super::*;
use crate::store::persistence::remote_control::ChannelLinkCounts;

const MIN_CHANNELS: i64 = 5;

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: "remote_control_link",
        label: "Remote Control linking",
        state,
        detail,
        fix,
    }
}

pub(crate) fn remote_control_link_row(counts: ChannelLinkCounts) -> CapabilityRow {
    let ChannelLinkCounts { channels, linked } = counts;
    if channels == 0 {
        return row(
            CapabilityState::Active,
            "No Remote Control channel opened in the last 24 hours, so there is nothing to judge linking by".to_owned(),
            String::new(),
        );
    }
    let detail = format!(
        "{linked} of {channels} Remote Control channels first seen in the last 24 hours were linked to a local session"
    );
    if channels >= MIN_CHANNELS && linked * 2 < channels {
        return row(
            CapabilityState::Degraded,
            format!(
                "{detail}. Push-wake and the launcher's channel check cannot reach an unlinked session; a Claude Code update may have changed the Remote Control protocol."
            ),
            "Run `make rc-protocol-check` to see which part of the protocol moved, and compare the client_version of recent channels with older ones.".to_owned(),
        );
    }
    row(CapabilityState::Active, detail, String::new())
}

pub(crate) async fn remote_control_link_capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is disabled, so Remote Control channels are not recorded".to_owned(),
            String::new(),
        );
    };
    match persist.recent_channel_link_counts().await {
        Ok(counts) => remote_control_link_row(counts),
        Err(e) => row(
            CapabilityState::Degraded,
            format!("Could not read recent Remote Control channels: {e}"),
            "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_for(channels: i64, linked: i64) -> CapabilityState {
        remote_control_link_row(ChannelLinkCounts { channels, linked }).state
    }

    #[test]
    fn link_row_with_no_channels_is_active() {
        assert_eq!(state_for(0, 0), CapabilityState::Active);
    }

    #[test]
    fn link_row_with_most_channels_linked_is_active() {
        assert_eq!(state_for(84, 80), CapabilityState::Active);
        assert_eq!(state_for(10, 5), CapabilityState::Active);
    }

    #[test]
    fn link_row_with_fewer_than_half_linked_is_degraded() {
        let r = remote_control_link_row(ChannelLinkCounts {
            channels: 10,
            linked: 0,
        });
        assert_eq!(r.state, CapabilityState::Degraded);
        assert!(r.detail.contains("0 of 10"), "{}", r.detail);
        assert!(r.fix.contains("rc-protocol-check"), "{}", r.fix);
    }

    #[test]
    fn link_row_with_too_few_channels_is_not_judged() {
        assert_eq!(state_for(4, 0), CapabilityState::Active);
    }
}
