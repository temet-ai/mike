use crate::store::persistence::fleet_repo::FleetPersist as _;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::base_state::BaseState;
use crate::web_base::{persist_error_response, persist_unavailable_response};

const CHANNEL_CLAIM_LIMIT: i64 = 20;

#[derive(Debug, Serialize)]
pub struct ClaimedChannelMessageDto {
    pub message_uuid: Uuid,
    pub from_session_uuid: Option<Uuid>,
    pub from_display_name: Option<String>,
    pub from_role: Option<String>,
    pub body: String,
    pub queued_at: String,
    pub delivery_attempts: i32,
    pub thread_uuid: Option<Uuid>,
    pub in_reply_to_uuid: Option<Uuid>,
    pub first_for_recipient: bool,
    pub envelope: String,
    pub from_peer_address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChannelClaimResponseDto {
    pub session_uuid: Uuid,
    pub polled_at: String,
    pub native_delivery: bool,
    pub messages: Vec<ClaimedChannelMessageDto>,
}

fn delivers_by_channel_by_default() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct ChannelClaimRequest {
    #[serde(default)]
    pub peer_address: Option<String>,
    #[serde(default = "delivers_by_channel_by_default")]
    pub channel: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct ClaimPlan {
    peer_address: Option<String>,
    native_delivery: bool,
    channel: bool,
}

impl ClaimPlan {
    fn for_request(request: &ChannelClaimRequest, native_peer_mail: bool) -> Self {
        let peer_address = crate::mcp_channel::valid_peer_address(request.peer_address.as_deref());
        let native_delivery = native_peer_mail && peer_address.is_some();
        Self {
            peer_address,
            native_delivery,
            channel: request.channel,
        }
    }

    fn claims(&self) -> bool {
        self.native_delivery || self.channel
    }
}

pub async fn claim_channel_messages(
    State(state): State<BaseState>,
    Path(session_uuid): Path<Uuid>,
    body: Option<Json<ChannelClaimRequest>>,
) -> Result<Json<ChannelClaimResponseDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let request = body.map(|Json(b)| b).unwrap_or(ChannelClaimRequest {
        peer_address: None,
        channel: true,
    });
    let plan = ClaimPlan::for_request(
        &request,
        state.feature_toggle_overlay.load().native_peer_mail,
    );
    let polled_at = chrono::Utc::now().to_rfc3339();
    if !plan.claims() {
        return Ok(Json(ChannelClaimResponseDto {
            session_uuid,
            polled_at,
            native_delivery: false,
            messages: Vec::new(),
        }));
    }

    crate::mcp_channel::listeners().mark_live(session_uuid);
    if let Some(address) = plan.peer_address {
        crate::mcp_channel::peer_addresses().record(session_uuid, address);
    }
    let native_delivery = plan.native_delivery;

    let claimed = handle
        .fleet()
        .claim_channel_messages(session_uuid, CHANNEL_CLAIM_LIMIT)
        .await
        .map_err(persist_error_response)?;

    let messages: Vec<_> = claimed.into_iter().map(claimed_message_dto).collect();

    Ok(Json(ChannelClaimResponseDto {
        session_uuid,
        polled_at,
        native_delivery,
        messages,
    }))
}

fn claimed_message_dto(
    row: crate::store::persistence::mailbox::ClaimedChannelMessageRow,
) -> ClaimedChannelMessageDto {
    let envelope = row.envelope();
    let queued_at = row.queued_at.to_rfc3339();
    let from_peer_address = row
        .from_session_uuid
        .and_then(|from| crate::mcp_channel::peer_addresses().address_of(from));
    ClaimedChannelMessageDto {
        from_peer_address,
        message_uuid: row.message_uuid,
        from_session_uuid: row.from_session_uuid,
        from_display_name: row.from_display_name,
        from_role: row.from_role,
        body: row.body,
        queued_at,
        delivery_attempts: row.delivery_attempts,
        thread_uuid: row.thread_uuid,
        in_reply_to_uuid: row.in_reply_to_uuid,
        first_for_recipient: row.first_for_recipient,
        envelope,
    }
}

#[derive(Debug, Deserialize)]
pub struct ChannelReleaseRequest {
    pub message_uuids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ChannelReleaseResponseDto {
    pub released: u64,
}

pub async fn release_channel_messages(
    State(state): State<BaseState>,
    Path(session_uuid): Path<Uuid>,
    Json(body): Json<ChannelReleaseRequest>,
) -> Result<Json<ChannelReleaseResponseDto>, Response> {
    let Some(handle) = state.persist() else {
        return Err(persist_unavailable_response());
    };
    let released = handle
        .fleet()
        .release_channel_messages(session_uuid, body.message_uuids)
        .await
        .map_err(persist_error_response)?;
    Ok(Json(ChannelReleaseResponseDto { released }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "uds:/run/user/1000/cc-socks/7.sock";

    fn request(peer_address: Option<&str>, channel: bool) -> ChannelClaimRequest {
        ChannelClaimRequest {
            peer_address: peer_address.map(str::to_string),
            channel,
        }
    }

    #[test]
    fn native_delivery_needs_the_setting_and_a_valid_address() {
        let on = ClaimPlan::for_request(&request(Some(ADDRESS), true), true);
        assert!(on.native_delivery);
        let off = ClaimPlan::for_request(&request(Some(ADDRESS), true), false);
        assert!(!off.native_delivery);
        let bad = ClaimPlan::for_request(&request(Some("uds:x"), true), true);
        assert!(!bad.native_delivery);
        assert_eq!(bad.peer_address, None);
    }

    #[test]
    fn a_session_with_no_channel_claims_nothing_while_the_setting_is_off() {
        assert!(!ClaimPlan::for_request(&request(Some(ADDRESS), false), false).claims());
        assert!(ClaimPlan::for_request(&request(Some(ADDRESS), false), true).claims());
        assert!(ClaimPlan::for_request(&request(None, true), false).claims());
    }

    #[test]
    fn an_old_client_body_claims_by_channel() -> anyhow::Result<()> {
        let old: ChannelClaimRequest = serde_json::from_str("{}")?;
        assert_eq!(
            ClaimPlan::for_request(&old, true),
            ClaimPlan {
                peer_address: None,
                native_delivery: false,
                channel: true
            }
        );
        Ok(())
    }
}
