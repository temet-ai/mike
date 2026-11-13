//! A live verdict and its projection onto the turn, in one transaction: the
//! verdict is eval's, the projection step is the base's (operator_turns).

use super::TurnVerdictInsert;
use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::projection::{
    ProjectionParams, PromoteOutcome, promote_on,
};

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn insert_live_verdict_and_promote(
        self,
        verdict: TurnVerdictInsert<'_>,
        params: &ProjectionParams,
    ) -> Result<(i64, PromoteOutcome), PersistError> {
        crate::metrics::time_db("insert_live_verdict_and_promote", async move {
            let projected =
                verdict.mode.as_str() == "live" && verdict.bundle_id == Some(params.live_bundle_id);
            let mut tx = self.pool.begin().await?;
            let verdict_id = super::insert_turn_verdict_on(&mut tx, &verdict, projected).await?;
            let outcome = promote_on(&mut tx, verdict_id, params).await?;
            tx.commit().await?;
            Ok((verdict_id, outcome))
        })
        .await
    }
}
