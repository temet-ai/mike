use chrono::Utc;

use mike::store::persistence::rehydrate::{
    PROMPT_ROSTER_LIMIT, fetch_session_model_breakdown, fetch_session_prompt_roster,
    fetch_session_summary,
};

use crate::store::rehydrate::support::{
    SeedRequest, isolated_pool, seed_project, seed_request, seed_session, shared_pool, unique,
};

#[tokio::test]
async fn prompt_roster_decodes_composition_id_via_bigint_join() -> anyhow::Result<()> {
    let Some((pool, _db)) = isolated_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("roster-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();
    let req_id = seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base,
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let sha = uuid::Uuid::new_v4().simple().to_string() + &"a".repeat(32);
    let system_prompt_id: i64 = sqlx::query_scalar(
        "INSERT INTO system_prompts.system_prompts (prompt_sha256, blocks)
         VALUES ($1, '[]'::jsonb)
         RETURNING id",
    )
    .bind(&sha)
    .fetch_one(&pool)
    .await?;

    let comp_uuid: uuid::Uuid = uuid::Uuid::new_v4();
    let comp_id: i64 = sqlx::query_scalar(
        "INSERT INTO overrides.overrides
            (uuid, prompt_sha256, system_prompt_id, label, blocks)
         VALUES ($1, $2, $3, 'roster-label', '[]'::jsonb)
         RETURNING id",
    )
    .bind(comp_uuid)
    .bind(&sha)
    .bind(system_prompt_id)
    .fetch_one(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO requests.system_prompts
            (request_id, prompt_sha256, override_id, system_prompt_id,
             override_applied, filter_modified)
         VALUES ($1, $2, $3, $4, true, false)",
    )
    .bind(req_id)
    .bind(&sha)
    .bind(comp_id)
    .bind(system_prompt_id)
    .execute(&pool)
    .await?;

    let (rows, truncated) = fetch_session_prompt_roster(&pool, session_fk).await?;
    assert!(!truncated, "single prompt class must not trip the cap");
    assert_eq!(rows.len(), 1, "one distinct prompt class expected");
    let row = &rows[0];
    assert_eq!(row.prompt_sha256, sha);
    assert_eq!(
        row.composition_id,
        Some(comp_id),
        "composition id must decode from the pc.id projection"
    );
    assert_eq!(row.label.as_deref(), Some("roster-label"));
    assert_eq!(row.first_request_id, req_id);
    assert_eq!(row.occurrence_count, 1);

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn prompt_roster_truncation_flag_fires_over_cap() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("roster-cap-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    let over = PROMPT_ROSTER_LIMIT + 1;
    for i in 0..over {
        let created = base + chrono::Duration::microseconds(i as i64);
        let req_id = seed_request(
            &pool,
            SeedRequest {
                session_id_fk: session_fk,
                project_id,
                model: None,
                turn_origin: "primary",
                created_at: created,
                agent_instance_id: None,
                human_prompt: "",
            },
        )
        .await?;
        let sha = format!("{:032x}{i:032x}", uuid::Uuid::new_v4().as_u128());
        sqlx::query(
            "INSERT INTO requests.system_prompts
                (request_id, prompt_sha256, override_applied, filter_modified)
             VALUES ($1, $2, false, false)",
        )
        .bind(req_id)
        .bind(&sha)
        .execute(&pool)
        .await?;
    }

    let (rows, truncated) = fetch_session_prompt_roster(&pool, session_fk).await?;
    assert!(
        truncated,
        "cap must trip with PROMPT_ROSTER_LIMIT + 1 classes"
    );
    assert_eq!(
        rows.len(),
        PROMPT_ROSTER_LIMIT,
        "rows capped exactly at PROMPT_ROSTER_LIMIT"
    );
    assert!(rows.iter().all(|r| r.composition_id.is_none()));
    assert!(rows.iter().all(|r| r.label.is_none()));

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn model_breakdown_groups_orders_and_drops_nulls() -> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let project_id = seed_project(&pool, &unique("mb-proj")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), project_id).await?;
    let base = Utc::now();

    for i in 0..3 {
        seed_request(
            &pool,
            SeedRequest {
                session_id_fk: session_fk,
                project_id,
                model: Some("claude-opus-4-8"),
                turn_origin: "primary",
                created_at: base + chrono::Duration::microseconds(i),
                agent_instance_id: None,
                human_prompt: "",
            },
        )
        .await?;
    }
    seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: Some("claude-haiku-4-5"),
            turn_origin: "primary",
            created_at: base + chrono::Duration::microseconds(10),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;
    seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id,
            model: None,
            turn_origin: "primary",
            created_at: base + chrono::Duration::microseconds(20),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let rows = fetch_session_model_breakdown(&pool, session_fk).await?;
    assert_eq!(rows.len(), 2, "NULL-model request must be excluded");
    assert_eq!(rows[0].model, "claude-opus-4-8");
    assert_eq!(rows[0].count, 3);
    assert_eq!(rows[1].model, "claude-haiku-4-5");
    assert_eq!(rows[1].count, 1);

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn fetch_session_summary_projects_touched_reflects_request_level_divergence()
-> anyhow::Result<()> {
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };

    let session_project = seed_project(&pool, &unique("issue32-detail-session-home")).await?;
    let nested = seed_project(&pool, &unique("issue32-detail-nested")).await?;
    let session_fk = seed_session(&pool, uuid::Uuid::new_v4(), session_project).await?;
    seed_request(
        &pool,
        SeedRequest {
            session_id_fk: session_fk,
            project_id: nested,
            model: None,
            turn_origin: "primary",
            created_at: Utc::now(),
            agent_instance_id: None,
            human_prompt: "",
        },
    )
    .await?;

    let row = fetch_session_summary(&pool, session_fk)
        .await?
        .ok_or_else(|| anyhow::anyhow!("seeded session must be found"))?;

    assert!(row.project_path.starts_with("issue32-detail-session-home"));
    assert_eq!(row.projects_touched.len(), 1);
    assert!(
        row.projects_touched[0].starts_with("issue32-detail-nested"),
        "single-session detail path must report the request-level project, \
         not the session-level one"
    );

    pool.close().await;
    Ok(())
}
