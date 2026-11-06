use chrono::Duration;
use mike::dossier::render::render_inputs;
use mike::dossier::view::types::{AsOf, Audience, Budget, Needs, SessionSlices};

use super::{after, seeded};
use crate::seed::database_now;
use crate::turn_facts::fixture;

#[tokio::test]
async fn two_renders_of_the_same_moment_have_the_same_digest() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seeded(&f).await?;
    let at = chrono::Utc::now();
    let audience = Audience::SessionJudge {
        slices: SessionSlices::default(),
    };
    let load = || async {
        mike::dossier::view::load(
            &f.handle,
            f.session_id,
            AsOf::knowledge(at),
            Needs::for_audience(&audience),
        )
        .await
    };
    let a = render_inputs(&load().await?, &audience, Budget::Chars(24_000))?;
    let b = render_inputs(&load().await?, &audience, Budget::Chars(24_000))?;
    assert_eq!(a.inputs_digest, b.inputs_digest);
    assert_eq!(a.text, b.text);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn knowledge_mode_hides_an_entry_recorded_after_the_moment() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seeded(&f).await?;
    let now = database_now(f.handle.pool()).await?;
    let knowing = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(now),
        Needs::everything(None),
    )
    .await?;
    assert!(!knowing.entries.is_empty(), "the build recorded directives");
    let before_recorded = now - Duration::hours(1);
    let earlier = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::knowledge(before_recorded.max(after(&f, 250))),
        Needs::everything(None),
    )
    .await?;
    assert!(
        earlier.entries.is_empty(),
        "entries recorded at build time are absent before it in knowledge mode"
    );
    let reconstructed = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        AsOf::reconstructed_for_eval(after(&f, 250)),
        Needs::everything(None),
    )
    .await?;
    assert!(
        !reconstructed.entries.is_empty(),
        "reconstructed mode admits capture entries whose events came before T"
    );
    f.db.cleanup().await?;
    Ok(())
}
