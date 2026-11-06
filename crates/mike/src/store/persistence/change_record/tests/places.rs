use super::fixture;
use super::seed::{CHECKOUT, project, session};

#[tokio::test]
async fn change_place_is_zero_until_moved_and_never_moves_backwards() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let pool = f.handle.pool();
    let project_id = project(pool).await?;
    let one = session(pool, project_id, None).await?;
    let two = session(pool, project_id, None).await?;
    assert_eq!(f.handle.change_place(one, CHECKOUT).await?, 0);

    f.handle.move_change_place(one, CHECKOUT, 40).await?;
    assert_eq!(f.handle.change_place(one, CHECKOUT).await?, 40);
    f.handle.move_change_place(one, CHECKOUT, 10).await?;
    assert_eq!(f.handle.change_place(one, CHECKOUT).await?, 40);
    f.handle.move_change_place(one, CHECKOUT, 55).await?;
    assert_eq!(f.handle.change_place(one, CHECKOUT).await?, 55);

    assert_eq!(f.handle.change_place(two, CHECKOUT).await?, 0);
    assert_eq!(f.handle.change_place(one, "/work/other").await?, 0);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn move_change_place_for_unknown_session_is_an_error() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let project_id = project(f.handle.pool()).await?;
    let real = session(f.handle.pool(), project_id, None).await?;
    assert!(
        f.handle
            .move_change_place(real + 500, CHECKOUT, 3)
            .await
            .is_err()
    );
    f.db.cleanup().await?;
    Ok(())
}
