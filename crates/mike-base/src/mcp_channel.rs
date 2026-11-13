use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

const CHANNEL_LIVE_TTL: Duration = Duration::from_secs(15);
const CHANNEL_STALE_EVICT: Duration = Duration::from_secs(600);
const CHANNEL_TAKES_MAIL_WITHIN: Duration = Duration::from_secs(10);

fn within(elapsed: Duration, bound: Duration) -> bool {
    elapsed < bound
}

#[derive(Clone, Default)]
pub struct ChannelListeners(Arc<DashMap<Uuid, Instant>>);

static LISTENERS: LazyLock<ChannelListeners> = LazyLock::new(ChannelListeners::default);

pub fn listeners() -> &'static ChannelListeners {
    &LISTENERS
}

impl ChannelListeners {
    pub fn mark_live(&self, session_uuid: Uuid) {
        let now = Instant::now();
        self.0.insert(session_uuid, now);
        self.0
            .retain(|_, seen_at| within(now.duration_since(*seen_at), CHANNEL_STALE_EVICT));
    }

    pub fn is_live(&self, session_uuid: Uuid) -> bool {
        self.0
            .get(&session_uuid)
            .is_some_and(|seen_at| within(seen_at.elapsed(), CHANNEL_LIVE_TTL))
    }
}

const PEER_ADDRESS_MAX_LEN: usize = 512;

pub fn valid_peer_address(raw: Option<&str>) -> Option<String> {
    let address = raw?.trim();
    let path = address.strip_prefix("uds:")?;
    let shaped = path.starts_with('/')
        && address.len() <= PEER_ADDRESS_MAX_LEN
        && !address
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '"' | '<' | '>'));
    shaped.then(|| address.to_string())
}

#[derive(Clone, Default)]
pub struct PeerAddresses(Arc<DashMap<Uuid, (String, Instant)>>);

static PEER_ADDRESSES: LazyLock<PeerAddresses> = LazyLock::new(PeerAddresses::default);

pub fn peer_addresses() -> &'static PeerAddresses {
    &PEER_ADDRESSES
}

impl PeerAddresses {
    pub fn record(&self, session_uuid: Uuid, address: String) {
        let now = Instant::now();
        self.0.insert(session_uuid, (address, now));
        self.0
            .retain(|_, (_, seen_at)| within(now.duration_since(*seen_at), CHANNEL_STALE_EVICT));
    }

    pub fn address_of(&self, session_uuid: Uuid) -> Option<String> {
        self.0
            .get(&session_uuid)
            .filter(|entry| within(entry.1.elapsed(), CHANNEL_LIVE_TTL))
            .map(|entry| entry.0.clone())
    }
}

#[derive(Clone, Default)]
pub struct MailLeftToTheChannel(Arc<DashMap<Uuid, Instant>>);

static LEFT_TO_THE_CHANNEL: LazyLock<MailLeftToTheChannel> =
    LazyLock::new(MailLeftToTheChannel::default);

pub fn mail_left_to_the_channel() -> &'static MailLeftToTheChannel {
    &LEFT_TO_THE_CHANNEL
}

impl MailLeftToTheChannel {
    pub fn still_has_time(&self, message_uuid: Uuid) -> bool {
        self.still_has_time_at(message_uuid, Instant::now())
    }

    fn still_has_time_at(&self, message_uuid: Uuid, now: Instant) -> bool {
        let first_seen = *self.0.entry(message_uuid).or_insert(now);
        self.0
            .retain(|_, seen_at| within(now.duration_since(*seen_at), CHANNEL_STALE_EVICT));
        within(now.duration_since(first_seen), CHANNEL_TAKES_MAIL_WITHIN)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn mail_is_left_to_the_channel_for_ten_seconds_from_the_first_time_it_is_seen() {
        let left = MailLeftToTheChannel::default();
        let (mail, start) = (Uuid::new_v4(), Instant::now());
        assert!(left.still_has_time_at(mail, start));
        assert!(left.still_has_time_at(mail, start + Duration::from_secs(9)));
        assert!(
            !left.still_has_time_at(mail, start + Duration::from_secs(10)),
            "after ten seconds the request route takes the mail"
        );
        assert!(left.still_has_time_at(Uuid::new_v4(), start + Duration::from_secs(10)));
    }

    #[test]
    fn within_bound_holds_for_a_shorter_elapsed() {
        assert!(within(Duration::from_secs(1), Duration::from_secs(2)));
    }

    #[test]
    fn within_bound_fails_once_elapsed_reaches_the_bound() {
        assert!(!within(Duration::from_secs(2), Duration::from_secs(2)));
        assert!(!within(Duration::from_secs(3), Duration::from_secs(2)));
    }

    #[test]
    fn unmarked_session_is_not_live() {
        let listeners = ChannelListeners::default();
        assert!(!listeners.is_live(Uuid::new_v4()));
    }

    #[test]
    fn marked_session_is_live_immediately() {
        let listeners = ChannelListeners::default();
        let session = Uuid::new_v4();
        listeners.mark_live(session);
        assert!(listeners.is_live(session));
    }

    #[test]
    fn other_sessions_stay_unaffected_by_a_mark() {
        let listeners = ChannelListeners::default();
        let marked = Uuid::new_v4();
        let other = Uuid::new_v4();
        listeners.mark_live(marked);
        assert!(listeners.is_live(marked));
        assert!(!listeners.is_live(other));
    }

    #[test]
    fn peer_address_must_be_an_absolute_uds_path() {
        assert_eq!(
            valid_peer_address(Some("uds:/run/user/1000/cc-socks/42.sock")),
            Some("uds:/run/user/1000/cc-socks/42.sock".to_string())
        );
        assert_eq!(
            valid_peer_address(Some("/run/user/1000/cc-socks/42.sock")),
            None
        );
        assert_eq!(valid_peer_address(Some("uds:relative.sock")), None);
        assert_eq!(valid_peer_address(Some("bridge:abc")), None);
        assert_eq!(valid_peer_address(None), None);
    }

    #[test]
    fn peer_address_with_markup_or_spaces_is_refused() {
        assert_eq!(valid_peer_address(Some("uds:/tmp/a\".sock")), None);
        assert_eq!(valid_peer_address(Some("uds:/tmp/a<b>.sock")), None);
        assert_eq!(valid_peer_address(Some("uds:/tmp/a b.sock")), None);
        let long = format!("uds:/{}", "a".repeat(PEER_ADDRESS_MAX_LEN));
        assert_eq!(valid_peer_address(Some(&long)), None);
    }

    #[test]
    fn recorded_peer_address_is_returned_for_that_session_only() {
        let addresses = PeerAddresses::default();
        let (session, other) = (Uuid::new_v4(), Uuid::new_v4());
        addresses.record(session, "uds:/tmp/s.sock".to_string());
        assert_eq!(
            addresses.address_of(session),
            Some("uds:/tmp/s.sock".to_string())
        );
        assert_eq!(addresses.address_of(other), None);
    }

    #[test]
    fn marking_a_session_again_keeps_only_one_entry() {
        let listeners = ChannelListeners::default();
        let session = Uuid::new_v4();
        listeners.mark_live(session);
        listeners.mark_live(session);
        assert_eq!(listeners.0.len(), 1);
    }
}
