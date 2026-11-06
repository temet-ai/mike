use super::allocate::{AllocSection, SectionBody, allocate};
use super::cut::char_count;

fn sections(scale: usize) -> Vec<AllocSection> {
    vec![
        AllocSection {
            id: "ending",
            heading: "THE TURN'S ENDING:".to_owned(),
            body: SectionBody::Tail {
                text: "the assistant wrote a long ending. ".repeat(scale * 40),
            },
        },
        AllocSection {
            id: "asks",
            heading: "EARLIER OPERATOR MESSAGES:".to_owned(),
            body: SectionBody::List {
                items: (0..scale * 10)
                    .map(|i| format!("ask {i}: {}", "please refactor the importer. ".repeat(12)))
                    .collect(),
                noun: "operator messages are",
            },
        },
        AllocSection {
            id: "directives",
            heading: "STANDING DIRECTIVES:".to_owned(),
            body: SectionBody::List {
                items: vec!["- commit allow: \"please commit\"".to_owned()],
                noun: "directives are",
            },
        },
        AllocSection {
            id: "corrections",
            heading: "EARLIER CORRECTIONS:".to_owned(),
            body: SectionBody::List {
                items: Vec::new(),
                noun: "corrections are",
            },
        },
    ]
}

#[test]
fn output_fits_the_budget_and_every_section_is_reported() {
    for budget in [8_000, 24_000, 60_000] {
        for scale in [1, 5, 40] {
            let out = allocate(&sections(scale), budget, &[]);
            assert!(
                char_count(&out.text) <= budget,
                "budget {budget} scale {scale}: {} chars",
                char_count(&out.text)
            );
            let ids: Vec<&str> = out.reports.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, vec!["ending", "asks", "directives", "corrections"]);
            for report in &out.reports {
                assert!(
                    report.items_full + report.items_compressed <= report.items_total
                        || report.items_total == 0,
                    "{report:?}"
                );
            }
        }
    }
}

#[test]
fn a_cut_list_announces_what_it_dropped_and_keeps_the_newest() {
    let out = allocate(&sections(40), 8_000, &[]);
    assert!(out.text.contains("earlier operator messages are not shown"));
    assert!(out.text.contains("ask 399:"), "the newest ask is kept");
    assert!(out.text.contains("earlier characters not shown"));
    assert!(
        out.text.contains("(none)"),
        "an empty section renders (none)"
    );
}

#[test]
fn a_small_render_is_returned_whole() {
    let out = allocate(&sections(1), 60_000, &[]);
    assert!(!out.text.contains("not shown"));
    assert_eq!(out.reports[1].items_full, 10);
}

fn reactions(reply_chars: usize) -> Vec<AllocSection> {
    vec![AllocSection {
        id: "recent_reactions",
        heading: "RECENT OPERATOR REACTIONS (newest first):".to_owned(),
        body: SectionBody::NewestFirst {
            items: ["newest", "middle", "oldest"]
                .iter()
                .map(|which| format!("{which} reaction: {}", "x".repeat(reply_chars)))
                .collect(),
            noun: "reactions are",
        },
    }]
}

#[test]
fn a_list_that_runs_newest_first_is_cut_from_its_old_end() {
    let out = allocate(&reactions(400), 700, &[]);
    assert!(out.text.contains("newest reaction:"));
    assert!(!out.text.contains("oldest reaction:"));
    assert!(out.text.ends_with("(2 older reactions are not shown)"));
    assert!(char_count(&out.text) <= 700);
    assert_eq!(out.reports[0].items_full, 1);
}

#[test]
fn a_newest_first_list_too_long_for_one_item_keeps_the_start_of_the_newest() {
    let out = allocate(&reactions(4_000), 600, &[]);
    assert!(out.text.contains("newest reaction: xxx"));
    assert!(!out.text.contains("middle reaction:"));
    assert!(out.text.ends_with("(2 older reactions are not shown)"));
    assert!(char_count(&out.text) <= 600);
    assert_eq!(out.reports[0].items_compressed, 1);
}

#[test]
fn a_newest_first_list_that_fits_reads_the_same_as_any_list() {
    let newest_first = allocate(&reactions(40), 6_000, &[]);
    let mut plain = reactions(40);
    if let SectionBody::NewestFirst { items, noun } = plain[0].body.clone() {
        plain[0].body = SectionBody::List { items, noun };
    }
    assert_eq!(newest_first.text, allocate(&plain, 6_000, &[]).text);
}
