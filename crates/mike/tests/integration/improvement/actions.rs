use super::fixture;

async fn seed_daily_lesson(f: &super::Fixture, text: &str) -> anyhow::Result<i64> {
    let lesson_id: i64 = sqlx::query_scalar(
        "INSERT INTO memory.lessons (project_id, kind_id, text, stage_id, source_id)
         SELECT $1, lk.id, $2, ls.id, lsrc.id
           FROM memory.lesson_kinds lk, memory.lesson_stages ls, memory.lesson_sources lsrc
          WHERE lk.code = 'correction' AND ls.code = 'daily' AND lsrc.code = 'judge'
         RETURNING id",
    )
    .bind(f.project_id)
    .bind(text)
    .fetch_one(f.handle.pool())
    .await?;
    Ok(lesson_id)
}

struct RateRow<'a> {
    rollup_id: i64,
    population: &'a str,
    dimension: &'a str,
    dimension_value: &'a str,
    k: i32,
    n: i32,
}

async fn insert_rate_row(pool: &sqlx::PgPool, row: RateRow<'_>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO eval.rates (rollup_id, family, metric, population, scope_kind,
                                 window_start, window_end, window_days, role, axis, class,
                                 dimension, dimension_value, k, n, value, ci_low, ci_high,
                                 weighted, detail)
         VALUES ($1, 'action_effect', 'share', $2, 'global',
                 now() - interval '14 days', now(), 14, 'turn_judge', 'reaction', 'corrected',
                 $3, $4, $5, $6, $5::float8 / nullif($6, 0), 0, 1, false, '{}'::jsonb)",
    )
    .bind(row.rollup_id)
    .bind(row.population)
    .bind(row.dimension)
    .bind(row.dimension_value)
    .bind(row.k)
    .bind(row.n)
    .execute(pool)
    .await?;
    Ok(())
}

async fn claim_rollup(pool: &sqlx::PgPool) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO eval.rollups (for_day, sql_version, mode, labels_as_of)
         VALUES (current_date, 'action-effect-v1-test', 'manual', now())
         ON CONFLICT (for_day, sql_version) DO UPDATE SET mode = eval.rollups.mode
         RETURNING id",
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

#[tokio::test]
async fn get_lesson_action_decodes_a_real_int4_rate_row() -> anyhow::Result<()> {
    let Some(f) = fixture("improvement-actions-lesson").await? else {
        return Ok(());
    };
    let lesson_id = seed_daily_lesson(&f, "always run the narrow check first").await?;
    let rollup_id = claim_rollup(f.handle.pool()).await?;
    let lesson_id_text = lesson_id.to_string();
    insert_rate_row(
        f.handle.pool(),
        RateRow {
            rollup_id,
            population: "before",
            dimension: "lesson",
            dimension_value: &lesson_id_text,
            k: 3,
            n: 20,
        },
    )
    .await?;
    insert_rate_row(
        f.handle.pool(),
        RateRow {
            rollup_id,
            population: "after",
            dimension: "lesson",
            dimension_value: &lesson_id_text,
            k: 1,
            n: 20,
        },
    )
    .await?;

    let Some(row) = f.handle.get_lesson_action(lesson_id).await? else {
        anyhow::bail!("lesson action row must exist for a lesson with rate rows");
    };
    assert_eq!(row.k_before, Some(3));
    assert_eq!(row.n_before, Some(20));
    assert_eq!(row.k_after, Some(1));
    assert_eq!(row.n_after, Some(20));
    Ok(())
}

#[tokio::test]
async fn list_guard_question_actions_decodes_a_real_int4_rate_row() -> anyhow::Result<()> {
    let Some(f) = fixture("improvement-actions-guard").await? else {
        return Ok(());
    };
    let rollup_id = claim_rollup(f.handle.pool()).await?;
    insert_rate_row(
        f.handle.pool(),
        RateRow {
            rollup_id,
            population: "holdback",
            dimension: "guard_question",
            dimension_value: "relapse",
            k: 5,
            n: 34,
        },
    )
    .await?;
    insert_rate_row(
        f.handle.pool(),
        RateRow {
            rollup_id,
            population: "treated",
            dimension: "guard_question",
            dimension_value: "relapse",
            k: 2,
            n: 34,
        },
    )
    .await?;

    let rows = f.handle.list_guard_question_actions().await?;
    let Some(relapse) = rows.iter().find(|r| r.question == "relapse") else {
        anyhow::bail!(
            "relapse question policy row must exist (seeded by the turn_guard migration)"
        );
    };
    assert_eq!(relapse.k_holdback, Some(5));
    assert_eq!(relapse.n_holdback, Some(34));
    assert_eq!(relapse.k_treated, Some(2));
    assert_eq!(relapse.n_treated, Some(34));
    Ok(())
}
