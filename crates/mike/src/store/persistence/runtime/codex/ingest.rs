use uuid::Uuid;

use crate::runtime::codex::ingest::{FrameRoute, IngestFrame};
use crate::store::persistence::{PersistError, PersistHandle};

use super::{
    ClassificationProvenance, CodexCaptureOutcome, CodexIntakeContext, CodexProjectionApplication,
    CodexProjectionApplyOutcome, CodexReceiptInput,
};

impl PersistHandle {
    pub async fn capture_codex_frame(
        &self,
        frame: &IngestFrame,
    ) -> Result<CodexCaptureOutcome, PersistError> {
        let provenance = match frame.route {
            FrameRoute::Response => ClassificationProvenance::ReviewedRpcResponse,
            FrameRoute::Notification | FrameRoute::ServerRequest => {
                ClassificationProvenance::ReviewedNotification
            }
            FrameRoute::Unparsed => ClassificationProvenance::ReviewedOrdinaryRoute,
        };
        let context = CodexIntakeContext::new(
            frame.intake_context(),
            frame.connection_epoch,
            frame.frame_ordinal,
            provenance,
        )?;
        self.capture_codex_receipt(CodexReceiptInput {
            envelope: frame.observation.envelope.clone(),
            raw_payload: frame.observation.raw_payload.clone(),
            context,
        })
        .await
    }

    pub async fn drain_codex_projection(
        &self,
        generation_id: Uuid,
    ) -> Result<Vec<CodexProjectionApplication>, PersistError> {
        let mut applied = Vec::new();
        loop {
            match self.apply_next_codex_projection(generation_id).await? {
                CodexProjectionApplyOutcome::Applied(application) => applied.push(application),
                CodexProjectionApplyOutcome::CaughtUp(_) => return Ok(applied),
            }
        }
    }
}
