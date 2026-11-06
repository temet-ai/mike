use chrono::Duration;
use mike::dossier::render::steerer::render_steerer;
use mike::dossier::view::types::{AsOf, Audience, Needs, SteererSection};

use super::seeded;
use crate::turn_facts::fixture;

fn every_section() -> Vec<SteererSection> {
    vec![
        SteererSection::Ending,
        SteererSection::RecentReactions,
        SteererSection::Directives { classes: None },
        SteererSection::Promises,
        SteererSection::SteeringFacts,
        SteererSection::Asks,
        SteererSection::Corrections,
        SteererSection::SimilarCorrections,
        SteererSection::SimilarDirectives,
        SteererSection::SimilarLessons,
        SteererSection::EarlierNotes,
        SteererSection::TurnOpening,
        SteererSection::EarlierTurns,
    ]
}

#[tokio::test]
async fn a_section_loaded_with_only_what_it_reads_matches_the_full_dossier() -> anyhow::Result<()> {
    let Some(f) = fixture().await? else {
        return Ok(());
    };
    let (_, _, third) = seeded(&f).await?;
    let as_of = AsOf::knowledge(chrono::Utc::now());
    let window = Some(Duration::minutes(60));
    let full = mike::dossier::view::load(
        &f.handle,
        f.session_id,
        as_of,
        Needs {
            notes_window: window,
            ..Needs::for_audience(&Audience::Steerer {
                turn_id: Some(third),
                sections: Vec::new(),
            })
        },
    )
    .await?;
    for section in every_section() {
        let one = std::slice::from_ref(&section);
        let narrow = mike::dossier::view::load(
            &f.handle,
            f.session_id,
            as_of,
            Needs::for_sections(Some(third), one, window),
        )
        .await?;
        assert_eq!(
            render_steerer(&narrow, Some(third), one, 24_000)
                .allocated
                .text,
            render_steerer(&full, Some(third), one, 24_000)
                .allocated
                .text,
            "{section:?}"
        );
    }
    let rendered = render_steerer(&full, Some(third), &every_section(), 24_000)
        .allocated
        .text;
    assert!(
        rendered.contains("Then commit when green."),
        "the seeded session has content to compare: {rendered}"
    );
    f.db.cleanup().await?;
    Ok(())
}
