use chrono::{DateTime, Utc};
use sqlx::postgres::PgArguments;
use sqlx::{Arguments, Postgres, QueryBuilder};

use super::eligibility::{BACKOFF_ELAPSED, LEASE_FREE, queued_without_verdict};
use crate::store::persistence::{PersistError, PersistHandle};

pub const JUDGE_LEASE_SECS: i32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimScope {
    ClosedSince(DateTime<Utc>),
    Session(i64),
    ClosedBetween {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    },
}

fn lease_claim_query(
    bundle_id: i64,
    scope: ClaimScope,
    limit: i64,
) -> Result<QueryBuilder<Postgres>, PersistError> {
    let mut arguments = PgArguments::default();
    arguments.add(bundle_id).map_err(sqlx::Error::Encode)?;
    let mut query =
        QueryBuilder::with_arguments("WITH picked AS (SELECT t.id, t.closed_at ", arguments);
    query
        .push(queued_without_verdict())
        .push(" ")
        .push(BACKOFF_ELAPSED)
        .push(" ")
        .push(LEASE_FREE);
    match scope {
        ClaimScope::ClosedSince(since) => {
            query.push(" AND t.closed_at >= ").push_bind(since);
        }
        ClaimScope::Session(session_db_id) => {
            query.push(" AND t.session_id = ").push_bind(session_db_id);
        }
        ClaimScope::ClosedBetween { since, until } => {
            query
                .push(" AND t.closed_at >= ")
                .push_bind(since)
                .push(" AND t.closed_at < ")
                .push_bind(until);
        }
    }
    query
        .push(" ORDER BY t.closed_at ASC, t.id ASC LIMIT ")
        .push_bind(limit)
        .push(
            " FOR UPDATE OF t SKIP LOCKED
        ), leased AS (
            UPDATE memory.operator_turns u
               SET judge_claimed_until = now() + make_interval(secs => ",
        )
        .push_bind(JUDGE_LEASE_SECS)
        .push(
            "::int)
              FROM picked
             WHERE u.id = picked.id
            RETURNING u.id
        )
        SELECT picked.id, picked.closed_at
          FROM picked
          JOIN leased ON leased.id = picked.id
         ORDER BY picked.closed_at ASC, picked.id ASC",
        );
    Ok(query)
}

impl PersistHandle {
    pub async fn claim_judge_turns(
        &self,
        bundle_id: i64,
        scope: ClaimScope,
        limit: i64,
    ) -> Result<Vec<i64>, PersistError> {
        let claimed = self
            .claim_judge_turns_dated(bundle_id, scope, limit)
            .await?;
        Ok(claimed.into_iter().map(|(id, _)| id).collect())
    }

    pub async fn claim_judge_turns_dated(
        &self,
        bundle_id: i64,
        scope: ClaimScope,
        limit: i64,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        Ok(lease_claim_query(bundle_id, scope, limit)?
            .build_query_as::<(i64, DateTime<Utc>)>()
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn renew_judge_leases(&self, turn_ids: &[i64]) -> Result<u64, PersistError> {
        crate::metrics::time_db("renew_judge_leases", async move {
            let renewed = sqlx::query!(
                "UPDATE memory.operator_turns
                    SET judge_claimed_until = now() + make_interval(secs => $2::int)
                  WHERE id = ANY($1) AND judge_claimed_until IS NOT NULL",
                turn_ids,
                JUDGE_LEASE_SECS,
            )
            .execute(&self.pool)
            .await?;
            Ok::<_, PersistError>(renewed.rows_affected())
        })
        .await
    }

    pub async fn release_judge_leases(&self, turn_ids: &[i64]) -> Result<u64, PersistError> {
        crate::metrics::time_db("release_judge_leases", async move {
            let released = sqlx::query!(
                "UPDATE memory.operator_turns
                    SET judge_claimed_until = NULL
                  WHERE id = ANY($1) AND judge_claimed_until IS NOT NULL",
                turn_ids,
            )
            .execute(&self.pool)
            .await?;
            Ok::<_, PersistError>(released.rows_affected())
        })
        .await
    }
}
