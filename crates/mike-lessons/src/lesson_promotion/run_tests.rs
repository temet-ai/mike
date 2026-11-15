use super::super::test_support::{Case, Home, lesson, lessons};
use super::*;

fn routing_with(standing: &[(&str, &str)]) -> Routing {
    Routing {
        routes: BTreeMap::new(),
        decisions: Vec::new(),
        standing: BTreeMap::new(),
        standing_checkouts: standing
            .iter()
            .map(|(file, checkout)| (PathBuf::from(file), PathBuf::from(checkout)))
            .collect(),
        coverage: Value::Null,
    }
}

#[test]
fn lessons_routed_standing_or_covered_are_left_out_of_every_skill() -> anyhow::Result<()> {
    let rows = vec![
        lesson(1, Some("shared:release")),
        lesson(2, Some("shared:release")),
        lesson(3, Some("shared:release")),
    ];
    let routes = BTreeMap::from([
        (1, LessonRoute::Standing),
        (2, LessonRoute::Covered),
        (3, LessonRoute::Injection),
    ]);
    let kept = injection_lessons(&rows, &routes);
    assert_eq!(kept.iter().map(|row| row.id).collect::<Vec<_>>(), vec![3]);
    let mut home = Home::new()?;
    home.run(Case::new(kept))?;
    let memory = std::fs::read_to_string(
        home.dir
            .path()
            .join("skills/shared-workflows/references/memory.md"),
    )?;
    assert!(memory.contains("[lesson:3 rev:1]"));
    assert!(!memory.contains("[lesson:1 rev:1]"));
    assert!(!memory.contains("[lesson:2 rev:1]"));
    Ok(())
}

#[test]
fn a_lesson_without_a_route_is_left_for_injection() {
    let rows = lessons("deploy", 1..3);
    assert_eq!(injection_lessons(&rows, &BTreeMap::new()).len(), 2);
}

#[test]
fn the_standing_file_is_excluded_unless_it_conflicts() {
    let plan = SkillPlan::default();
    let routing = routing_with(&[
        ("/work/a/.claude/rules/mike-lessons.md", "/work/a"),
        ("/work/b/.claude/rules/mike-lessons.md", "/work/b"),
    ]);
    let conflicts = Conflicts::from([(
        "/work/b/.claude/rules/mike-lessons.md".to_owned(),
        "edited".to_owned(),
    )]);
    let excludes = collect_excludes(&plan, &routing, &conflicts);
    assert_eq!(
        excludes,
        BTreeMap::from([(PathBuf::from("/work/a"), vec![STANDING_FILE.to_owned()])])
    );
}

#[test]
fn a_dry_run_names_a_checkout_without_a_git_directory_as_skipped_and_writes_nothing()
-> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let excludes = BTreeMap::from([(dir.path().to_path_buf(), vec!["x/".to_owned()])]);
    let skipped = apply_excludes(&excludes, true);
    assert_eq!(skipped, vec![dir.path().display().to_string()]);
    std::fs::create_dir_all(dir.path().join(".git"))?;
    assert!(apply_excludes(&excludes, false).is_empty());
    assert!(!dir.path().join(".git/info/exclude").exists());
    assert!(apply_excludes(&excludes, true).is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".git/info/exclude"))?,
        "x/\n"
    );
    Ok(())
}

#[test]
fn route_counts_split_the_decisions_by_route() {
    let mut routing = routing_with(&[]);
    for (id, route) in [
        (1, LessonRoute::Standing),
        (2, LessonRoute::Standing),
        (3, LessonRoute::Covered),
        (4, LessonRoute::Injection),
    ] {
        routing.decisions.push(super::super::routes::RouteDecision {
            lesson_id: id,
            project_id: 1,
            route,
            covered_by: None,
            revision: 1,
        });
    }
    let counts = route_counts(&routing);
    assert_eq!(counts["standing"], 2);
    assert_eq!(counts["covered"], 1);
    assert_eq!(counts["injection"], 1);
}
