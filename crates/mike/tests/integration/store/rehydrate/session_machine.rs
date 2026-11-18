use mike::store::persistence::rehydrate::{
    PageWindow, SessionListFilterParams, fetch_session_summary, list_sessions_paginated_filtered,
};
use mike::types::SortOrder;

use crate::store::rehydrate::support::{seed_project, seed_session, shared_pool, unique};

fn window() -> PageWindow {
    PageWindow {
        offset: 0,
        limit: 200,
        order: SortOrder::Desc,
        date_from: None,
        date_to: None,
        cursor: None,
    }
}

async fn listed_ids(pool: &sqlx::PgPool, machine: &str) -> anyhow::Result<Vec<String>> {
    let rows = list_sessions_paginated_filtered(
        pool,
        SessionListFilterParams {
            window: window(),
            search: None,
            min_request_count: None,
            project: None,
            machine: Some(machine),
        },
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.session_id).collect())
}

#[tokio::test]
async fn stamped_session_lists_its_machine_and_the_machine_filter_selects_it() -> anyhow::Result<()>
{
    let Some(pool) = shared_pool().await? else {
        return Ok(());
    };
    let machine = uuid::Uuid::new_v4();
    let name = unique("machine-filter");
    sqlx::query("INSERT INTO projects.machines (machine_uuid, name) VALUES ($1, $2)")
        .bind(machine)
        .bind(&name)
        .execute(&pool)
        .await?;
    let project_id = seed_project(&pool, &unique("machine-filter-proj")).await?;
    let stamped_uuid = uuid::Uuid::new_v4();
    let stamped = seed_session(&pool, stamped_uuid, project_id).await?;
    let unstamped_uuid = uuid::Uuid::new_v4();
    let unstamped = seed_session(&pool, unstamped_uuid, project_id).await?;
    sqlx::query("UPDATE sessions.sessions SET machine_uuid = $2 WHERE id = $1")
        .bind(stamped)
        .bind(machine)
        .execute(&pool)
        .await?;

    let by_name = listed_ids(&pool, &name.to_uppercase()).await?;
    assert_eq!(by_name, vec![stamped_uuid.to_string()]);
    let by_part_of_name = listed_ids(&pool, &name[2..]).await?;
    assert_eq!(by_part_of_name, vec![stamped_uuid.to_string()]);
    let by_uuid = listed_ids(&pool, &machine.to_string()).await?;
    assert_eq!(by_uuid, vec![stamped_uuid.to_string()]);
    assert!(
        listed_ids(&pool, &unique("no-such-machine"))
            .await?
            .is_empty()
    );

    let rows = list_sessions_paginated_filtered(
        &pool,
        SessionListFilterParams {
            window: window(),
            search: Some(&stamped_uuid.to_string()),
            min_request_count: None,
            project: None,
            machine: None,
        },
    )
    .await?;
    let row = rows
        .iter()
        .find(|row| row.session_id == stamped_uuid.to_string())
        .ok_or_else(|| anyhow::anyhow!("stamped session missing from the listing"))?;
    assert_eq!(row.machine_uuid, Some(machine));
    assert_eq!(row.machine_name.as_deref(), Some(name.as_str()));

    let summary = fetch_session_summary(&pool, stamped)
        .await?
        .ok_or_else(|| anyhow::anyhow!("stamped session has no summary"))?;
    assert_eq!(summary.machine_uuid, Some(machine));
    assert_eq!(summary.machine_name.as_deref(), Some(name.as_str()));

    let bare = fetch_session_summary(&pool, unstamped)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unstamped session has no summary"))?;
    assert_eq!(bare.machine_uuid, None);
    assert_eq!(bare.machine_name, None);
    Ok(())
}
