use std::fs;

use super::super::model::Recipe;
use super::super::test_support::{
    Case, Home, draft, key, lesson, lessons, other_uuid, project_uuid,
};
use super::*;

const DEPLOY: &str = "deploy-and-restart";

fn skill_path(home: &Home, folder: &str) -> PathBuf {
    home.skills().join(folder).join("SKILL.md")
}

fn read(path: &Path) -> anyhow::Result<String> {
    Ok(fs::read_to_string(path)?)
}

#[test]
fn a_topic_skill_lands_in_the_project_checkout_and_names_the_work() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut rows = vec![
        LessonRow {
            text: "Restart the proxy with start-proxy.sh.".to_owned(),
            ..lesson(1, Some(DEPLOY))
        },
        LessonRow {
            text: "Deploy only from the main checkout.".to_owned(),
            kind: "fact".to_owned(),
            ..lesson(2, Some(DEPLOY))
        },
        lesson(3, Some("lint")),
        lesson(4, Some("verifier:met")),
        lesson(5, Some("verifier:met")),
        lesson(6, None),
        lesson(7, None),
    ];
    for id in [8, 9] {
        rows.push(LessonRow {
            project_path: "/Users/dawid/absent-checkout".to_owned(),
            project_uuid: other_uuid(),
            ..lesson(id, Some(DEPLOY))
        });
    }
    let mut case = Case::new(rows);
    case.policies
        .insert(other_uuid().to_string(), "auto".to_owned());
    let outcome = home.run(case)?;
    let dir = home.skills().join(DEPLOY);
    let body = read(&dir.join("SKILL.md"))?;
    assert!(body.contains("name: deploy-and-restart\n"));
    assert!(body.contains("description: \"Deploy and restart: 2 accepted lessons for the `proj` project, kinds correction, fact. Use when running or touching `start-proxy.sh`, or when the task involves deploy and restart.\""));
    assert!(!body.contains("## Lesson"));
    let reference = read(&dir.join("references/lessons.md"))?;
    assert!(reference.contains("Lessons behind the deploy-and-restart skill for the `proj` project, each with its source turn."));
    assert!(reference.contains("## [lesson:1 rev:1] correction"));
    assert!(reference.contains("Restart the proxy with start-proxy.sh."));
    assert!(reference.contains("Deploy only from the main checkout."));
    assert_eq!(outcome.settled.authored, 0);
    assert_eq!(home.skill_names(), vec![DEPLOY.to_owned()]);
    assert_eq!(outcome.plan.emitted.len(), 1);
    assert_eq!(
        outcome.plan.skipped,
        BTreeSet::from(["/Users/dawid/absent-checkout".to_owned()])
    );
    assert!(!home.dir.path().join("skills").exists());
    Ok(())
}

#[test]
fn the_fallback_description_names_commands_and_files_from_the_lessons() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let rows = vec![
        LessonRow {
            text: "Never accept `cargo clippy -p` as the lint gate; run scripts/gate lint."
                .to_owned(),
            ..lesson(1, Some("rust-lint"))
        },
        LessonRow {
            text: "Use `cargo clippy -p` only for a quick look, then `make lint`.".to_owned(),
            ..lesson(2, Some("rust-lint"))
        },
    ];
    home.run(Case::new(rows))?;
    let body = read(&skill_path(&home, "rust-lint"))?;
    assert!(body.contains("Use when running or touching `cargo clippy -p`, `scripts/gate`, `make lint`, or when the task involves rust lint."));
    Ok(())
}

#[test]
fn a_checkout_is_found_through_a_location_this_machine_registered() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let rows: Vec<LessonRow> = (1..3)
        .map(|id| LessonRow {
            project_path: "/mnt/data/elsewhere/proj".to_owned(),
            ..lesson(id, Some("deploy"))
        })
        .collect();
    let mut case = Case::new(rows);
    case.locations.insert(
        project_uuid().to_string(),
        vec![home.checkout.display().to_string()],
    );
    let outcome = home.run(case)?;
    assert_eq!(outcome.plan.emitted.len(), 1);
    assert!(outcome.plan.skipped.is_empty());
    assert!(skill_path(&home, "deploy").exists());
    Ok(())
}

#[test]
fn an_agent_scoped_topic_skill_names_the_agent() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let rows: Vec<LessonRow> = (1..3)
        .map(|id| LessonRow {
            agent_id: Some(7),
            agent_name: Some("rust-developer".to_owned()),
            ..lesson(id, Some(DEPLOY))
        })
        .collect();
    home.run(Case::new(rows))?;
    assert_eq!(
        home.skill_names(),
        vec!["deploy-and-restart-rust-developer".to_owned()]
    );
    let body = read(&skill_path(&home, "deploy-and-restart-rust-developer"))?;
    assert!(body.contains("name: deploy-and-restart-rust-developer\n"));
    assert!(body.contains("Applies to the rust-developer agent only.\""));
    Ok(())
}

#[test]
fn agents_sharing_a_topic_share_one_skill_without_an_agent_suffix() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let rows = vec![
        LessonRow {
            agent_id: Some(7),
            agent_name: Some("rust-developer".to_owned()),
            ..lesson(1, Some("deploy"))
        },
        LessonRow {
            agent_id: Some(8),
            agent_name: Some("db-dev".to_owned()),
            ..lesson(2, Some("deploy"))
        },
    ];
    home.run(Case::new(rows))?;
    assert_eq!(home.skill_names(), vec!["deploy".to_owned()]);
    Ok(())
}

#[test]
fn only_lessons_placed_as_skills_or_unplaced_feed_a_skill() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut rows = Vec::new();
    for (topic, placement, ids) in [
        ("deploy", Some("memory"), 1..3),
        ("lint", Some("rule"), 3..5),
        ("review", Some("agent"), 5..7),
        ("gate", Some("nowhere"), 7..9),
        ("fleet", None, 9..11),
        ("testing", Some("skill"), 11..13),
    ] {
        for id in ids {
            rows.push(LessonRow {
                placement: placement.map(str::to_owned),
                ..lesson(id, Some(topic))
            });
        }
    }
    let outcome = home.run(Case::new(rows))?;
    assert_eq!(
        home.skill_names(),
        vec!["fleet".to_owned(), "testing".to_owned()]
    );
    assert_eq!(outcome.plan.emitted.len(), 2);
    Ok(())
}

#[test]
fn a_reviewed_skill_stays_when_its_lessons_are_placed_elsewhere() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut reviewed = draft(5, None, 2, "reviewed draft");
    reviewed.lesson_ids = vec![1, 2];
    let mut rows = vec![
        LessonRow {
            placement: Some("memory".to_owned()),
            ..lesson(1, Some(DEPLOY))
        },
        LessonRow {
            placement: Some("rule".to_owned()),
            ..lesson(2, Some(DEPLOY))
        },
    ];
    rows.extend((3..5).map(|id| LessonRow {
        placement: Some("memory".to_owned()),
        ..lesson(id, Some("unreviewed"))
    }));
    let mut case = Case::new(rows);
    case.drafts = vec![reviewed];
    home.run(case)?;
    assert_eq!(home.skill_names(), vec![DEPLOY.to_owned()]);
    assert!(read(&skill_path(&home, DEPLOY))?.contains("reviewed draft"));
    let reference = read(&home.skills().join(DEPLOY).join("references/lessons.md"))?;
    assert_eq!(reference.matches("## [lesson:").count(), 2);
    Ok(())
}

#[test]
fn lessons_covered_by_their_own_skill_keep_feeding_it() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut rows = Vec::new();
    for (topic, covered_by, ids) in [
        ("gate", "gate/SKILL.md", 1..3),
        ("lint", "gate/SKILL.md", 3..5),
        ("docs", "docs-style.md", 5..7),
    ] {
        for id in ids {
            rows.push(LessonRow {
                placement: Some("nowhere".to_owned()),
                covered_by: Some(covered_by.to_owned()),
                ..lesson(id, Some(topic))
            });
        }
    }
    home.run(Case::new(rows))?;
    assert_eq!(home.skill_names(), vec!["gate".to_owned()]);
    Ok(())
}

#[test]
fn a_topic_with_one_skill_lesson_among_memory_lessons_is_not_enough() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut rows = vec![lesson(1, Some("deploy"))];
    rows.extend((2..4).map(|id| LessonRow {
        placement: Some("memory".to_owned()),
        ..lesson(id, Some("deploy"))
    }));
    home.run(Case::new(rows))?;
    assert!(home.skill_names().is_empty());
    Ok(())
}

#[test]
fn two_topics_with_the_same_purpose_ship_one_skill() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut rows = lessons("deploying-the-proxy", 1..3);
    rows.extend(lessons("proxy-deploy-steps", 3..6));
    let outcome = home.run(Case::new(rows))?;
    assert_eq!(home.skill_names(), vec!["proxy-deploy-steps".to_owned()]);
    assert_eq!(
        outcome.plan.purpose_held,
        BTreeMap::from([(key("deploying-the-proxy"), key("proxy-deploy-steps"))])
    );
    Ok(())
}

#[test]
fn a_recipe_skill_lands_in_its_checkout_never_user_wide() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let mut case = Case::new(Vec::new());
    case.recipes = vec![
        Recipe {
            id: "outline".to_owned(),
            description: "Index headings".to_owned(),
            cwd: home.checkout.display().to_string(),
            contract: "read-only".to_owned(),
        },
        Recipe {
            id: "gone".to_owned(),
            description: "d".to_owned(),
            cwd: "/nonexistent/checkout".to_owned(),
            contract: "k".to_owned(),
        },
    ];
    let outcome = home.run(case)?;
    assert!(
        home.checkout
            .join(".claude/skills/tool-outline/SKILL.md")
            .exists()
    );
    assert!(
        home.checkout
            .join(".codex/skills/tool-outline/SKILL.md")
            .exists()
    );
    assert!(
        outcome
            .plan
            .desired
            .keys()
            .all(|path| path.starts_with(&home.checkout.display().to_string()))
    );
    assert!(!home.dir.path().join("skills").exists());
    assert_eq!(
        outcome.plan.excludes[&home.checkout],
        vec![
            ".claude/skills/tool-outline/".to_owned(),
            ".codex/skills/tool-outline/".to_owned()
        ]
    );
    Ok(())
}

#[test]
fn shared_lessons_write_the_shared_workflows_skill_into_every_skill_root() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let rows = vec![
        LessonRow {
            revision: 2,
            ..lesson(1, Some("shared:release"))
        },
        lesson(2, Some("shared:docs")),
    ];
    home.run(Case::new(rows))?;
    let skills = home.dir.path().join("skills/shared-workflows");
    assert!(read(&skills.join("SKILL.md"))?.contains("name: shared-workflows\n"));
    let memory = read(&skills.join("references/memory.md"))?;
    assert!(memory.contains("## [lesson:1 rev:2] correction (shared:release)"));
    assert!(!memory.contains("## Lesson"));
    Ok(())
}

#[test]
fn a_hand_edited_skill_is_a_conflict_and_is_left_as_edited() -> anyhow::Result<()> {
    let mut home = Home::new()?;
    let first = home.run(Case::new(lessons(DEPLOY, 1..3)))?;
    assert!(first.conflicts.is_empty());
    let path = skill_path(&home, DEPLOY);
    std::fs::write(&path, "edited by hand\n")?;
    let second = home.run(Case::new(lessons(DEPLOY, 1..4)))?;
    assert!(second.conflicts.contains_key(&path.display().to_string()));
    assert_eq!(read(&path)?, "edited by hand\n");
    Ok(())
}
