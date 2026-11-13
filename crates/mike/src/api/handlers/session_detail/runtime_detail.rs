use crate::store::persistence::runtime_repo::RuntimePersist as _;
use axum::http::StatusCode;
use serde::Serialize;
use uuid::Uuid;

use super::SessionDetail;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Serialize)]
pub(crate) struct ItemPage {
    items: Vec<crate::store::persistence::runtime::RuntimeItemRow>,
    offset: usize,
    limit: usize,
    has_more: bool,
}

fn unavailable(error: PersistError) -> StatusCode {
    tracing::warn!(error = %error, "session runtime observation read failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

pub(super) async fn attach(
    handle: &PersistHandle,
    id: &str,
    detail: &mut SessionDetail,
    offset: usize,
    limit: usize,
) -> Result<(), StatusCode> {
    let session = Uuid::parse_str(id).map_err(|_| StatusCode::NOT_FOUND)?;
    let ids = [session];
    let limit = limit.clamp(1, 100);
    let (observation, items) = tokio::join!(
        handle.runtime().session_observations(&ids),
        handle.runtime().list_runtime_items(
            session,
            limit as i64 + 1,
            i64::try_from(offset).unwrap_or(i64::MAX)
        ),
    );
    detail.observation = observation.map_err(unavailable)?.into_iter().next();
    let mut items = items.map_err(unavailable)?;
    let has_more = items.len() > limit;
    if has_more {
        items.pop();
    }
    detail.runtime_items = Some(ItemPage {
        items,
        offset,
        limit,
        has_more,
    });
    if let Some(observation) = &detail.observation {
        detail.summary.busy |= observation.lifecycle.as_deref() == Some("active");
    }
    Ok(())
}
