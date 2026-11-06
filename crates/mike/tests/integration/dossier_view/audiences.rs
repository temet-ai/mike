use mike::dossier::render::{TypedView, render};
use mike::dossier::view::types::{AsOf, Audience, Budget, SessionSlices, SteererSection};

use super::seeded;
use crate::seed::database_now;
use crate::turn_facts::fixture;

#[tokio::test]
async fn the_judge_render_carries_no_reaction_and_no_base_rate() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (_, _, third) = seeded(&f).await?;
    let rendered = render(
        &f.handle,
        f.session_id,
        AsOf::knowledge(chrono::Utc::now()),
        &Audience::Judge { turn_id: third },
        Budget::JudgeSlices,
    )
    .await?;
    assert!(!rendered.text.contains("reaction"));
    assert!(!rendered.text.to_lowercase().contains("base rate"));
    assert!(rendered.text.contains("Then commit when green."));
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_session_judge_render_lists_every_genuine_turn() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    seeded(&f).await?;
    let rendered = render(
        &f.handle,
        f.session_id,
        AsOf::knowledge(chrono::Utc::now()),
        &Audience::SessionJudge {
            slices: SessionSlices::default(),
        },
        Budget::Chars(24_000),
    )
    .await?;
    let turns = rendered
        .sections
        .iter()
        .find(|s| s.id == "turns")
        .ok_or_else(|| anyhow::anyhow!("no turns section"))?;
    assert_eq!(turns.items_total, 3);
    f.db.cleanup().await?;
    Ok(())
}

#[tokio::test]
async fn the_steerer_render_fills_its_typed_fields() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (_, _, third) = seeded(&f).await?;
    let rendered = render(
        &f.handle,
        f.session_id,
        AsOf::knowledge(database_now(f.handle.pool()).await?),
        &Audience::Steerer {
            turn_id: Some(third),
            sections: vec![
                SteererSection::RecentReactions,
                SteererSection::Directives { classes: None },
            ],
        },
        Budget::Chars(24_000),
    )
    .await?;
    let TypedView::Steerer(view) = rendered.typed else {
        anyhow::bail!("expected the steerer view");
    };
    assert!(!view.directives.is_empty());
    assert!(!view.recent_reactions.is_empty());
    assert_eq!(view.rendered, rendered.text);
    f.db.cleanup().await?;
    Ok(())
}
