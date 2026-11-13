use super::*;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::test_support;

#[tokio::test]
async fn exact_operation_repeats_include_non_cargo_commands_and_keep_arguments()
-> anyhow::Result<()> {
    let Some(db) = test_support::acquire_ephemeral_db().await? else {
        return Ok(());
    };
    let handle = PersistHandle::connect(&db.database_url).await?;
    let sql = include_str!("run_record_sql/cargo_repeats.sql");
    let (_, measured) = sql
        .split_once("runs AS (")
        .ok_or_else(|| anyhow::anyhow!("runs missing"))?;
    let query = format!(
        "WITH bash(seat,cmd) AS (VALUES
        ('one','npm run typecheck'),('two','npm run typecheck'),
        ('one','python3 check.py 1'),('two','python3 check.py 2'),
        ('two','echo npm run typecheck'),('one','cargo check')),
        runs AS ({measured}"
    );
    let row: (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(query.replace("$2", "$1")))
        .bind(r"^\s*(npm run typecheck|python3 check\.py [12])\s*$")
        .fetch_one(&handle.pool)
        .await?;
    assert_eq!(row, (4, 1));
    db.cleanup().await?;
    Ok(())
}

#[test]
fn stored_records_expose_bypass_diagnostics() {
    let row = operation_document(serde_json::json!({
        "measure_version": "run-record-v2-declared-operations",
        "cargo_commands": 4, "cargo_repeats_across_seats": 1
    }));
    assert_eq!(row["direct_operation_bypasses"], 4);
    assert_eq!(row["operation_repeats_across_seats"], 1);
    assert!(
        row["operation_adoption_action"]
            .as_str()
            .is_some_and(|v| v.contains("fleet_check"))
    );
}
