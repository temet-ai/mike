use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::runtime::RuntimeObservation;
use crate::runtime::codex::{
    ItemKey, ProjectedItem, ProjectionBatch, ProjectionStatus, decode_observation,
};
use crate::store::persistence::pool_classes::PoolClasses;
use crate::store::persistence::{PersistError, PersistHandle};

use super::{CodexIntakeContext, StoredRuntimeReceipt};

pub async fn lock_intake(
    connection: &mut PgConnection,
    namespace_id: Uuid,
) -> Result<i64, PersistError> {
    super::intake::lock_intake(connection, namespace_id).await
}

pub async fn insert_observation(
    connection: &mut PgConnection,
    observation: &RuntimeObservation,
) -> Result<i64, PersistError> {
    super::intake::insert_observation(connection, observation).await
}

pub async fn insert_context(
    connection: &mut PgConnection,
    event_id: i64,
    observation: &RuntimeObservation,
    context: &CodexIntakeContext,
    transport_gap: bool,
) -> Result<i64, PersistError> {
    super::intake::insert_context(connection, event_id, observation, context, transport_gap).await
}

pub fn decode_with_stored_context(
    observation: &RuntimeObservation,
    context: &CodexIntakeContext,
) -> ProjectionBatch {
    let scope = context.scope();
    decode_observation(observation, context.borrowed(scope.as_ref()))
}

pub const STATE_CODEC_VERSION: i32 = super::codec::STATE_CODEC_VERSION;

pub fn encode_item(item: &ProjectedItem) -> Result<String, PersistError> {
    super::codec::encode_item(item)
}

pub fn decode_item(document: &str, expected_key: &ItemKey) -> Result<ProjectedItem, PersistError> {
    super::codec::decode_item(document, expected_key)
}

pub fn validate_context(context: &CodexIntakeContext) -> Result<(), PersistError> {
    context.validate()
}

pub fn validate_batch(
    batch: &ProjectionBatch,
    receipt: &StoredRuntimeReceipt,
) -> Result<(), PersistError> {
    super::effects::validate_batch(batch, receipt)
}

pub fn parse_status(status: &str) -> Result<ProjectionStatus, PersistError> {
    super::types::parse_status(status)
}

pub fn validate_page(limit: i64) -> Result<(), PersistError> {
    super::reads::validate_page(limit)
}

pub fn handle_from_pool(pool: PgPool) -> PersistHandle {
    PersistHandle::from_pools(PoolClasses::single(pool))
}
