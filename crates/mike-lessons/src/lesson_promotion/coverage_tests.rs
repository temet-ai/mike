use std::fs;

use super::super::test_support::lesson;
use super::*;

#[test]
fn text_is_split_into_paragraphs_and_list_items_and_skips_headings_and_code() {
    let text = "# Rules\n\nAlways run the narrow check before committing a change.\nIt keeps the loop short.\n\n- Never force-push a shared branch without asking first.\n- Format files with rustfmt before the commit, using the project edition.\n- short\n\n```sh\nmake test everything and more and more\n```\n\n1. Keep commit messages free of attribution lines for the client.\n";
    let units = split_units(text);
    assert_eq!(
        units,
        vec![
            "Always run the narrow check before committing a change. It keeps the loop short.",
            "Never force-push a shared branch without asking first.",
            "Format files with rustfmt before the commit, using the project edition.",
            "Keep commit messages free of attribution lines for the client.",
        ]
    );
}

#[test]
fn cosine_is_one_for_equal_directions_and_zero_for_mismatched_lengths() {
    assert!((cosine(&[1.0, 2.0], &[2.0, 4.0]) - 1.0).abs() < 1e-9);
    assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-9);
    assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    assert_eq!(cosine(&[], &[]), 0.0);
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
}

#[test]
fn a_lesson_is_covered_by_the_most_similar_unit_at_or_above_the_threshold() {
    let units = vec![
        Unit {
            path: "CLAUDE.md".to_owned(),
            text: "a".to_owned(),
        },
        Unit {
            path: ".claude/rules/style.md".to_owned(),
            text: "b".to_owned(),
        },
    ];
    let unit_vectors = vec![vec![1.0, 0.0], vec![0.8, 0.6]];
    let lesson_vectors = BTreeMap::from([
        (1, vec![0.9, 0.5]),
        (2, vec![0.0, 1.0]),
        (3, vec![1.0, 0.0]),
    ]);
    let lessons = vec![lesson(1, None), lesson(2, None), lesson(4, None)];
    let covered = covered_lessons(
        &lessons,
        &Embedded {
            units: &units,
            unit_vectors: &unit_vectors,
            lesson_vectors: &lesson_vectors,
        },
        0.95,
    );
    assert_eq!(
        covered,
        BTreeMap::from([(1, ".claude/rules/style.md".to_owned())])
    );
}

#[test]
fn hand_written_units_read_the_three_places_and_skip_what_mike_writes() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let root = dir.path();
    fs::create_dir_all(root.join(".claude/rules/nested"))?;
    fs::write(
        root.join("CLAUDE.md"),
        "Always run the narrow check first, then commit.\n",
    )?;
    fs::write(
        root.join("AGENTS.md"),
        "Never push to main without a pull request.\n",
    )?;
    fs::write(
        root.join(".claude/rules/nested/style.md"),
        "Write no comments in Rust source, name things instead.\n",
    )?;
    fs::write(
        root.join(STANDING_FILE),
        "Generated lesson that must not count as hand written text.\n",
    )?;
    fs::write(
        root.join(".claude/rules/other.md"),
        format!("{MARKER}\nAnother generated file that is not hand written.\n"),
    )?;
    let units = hand_written_units(root, 1500);
    let paths: Vec<&str> = units.iter().map(|unit| unit.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["CLAUDE.md", "AGENTS.md", ".claude/rules/nested/style.md"]
    );
    Ok(())
}

#[test]
fn a_unit_is_cut_to_the_embedding_chunk_size() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    fs::write(
        dir.path().join("CLAUDE.md"),
        format!("{}\n", "word ".repeat(100)),
    )?;
    let units = hand_written_units(dir.path(), 50);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].text.chars().count(), 50);
    Ok(())
}
