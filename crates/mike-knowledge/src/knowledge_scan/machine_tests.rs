use std::path::Path;

use super::files::SourceKind;
use super::machine::{AGENT_MEMORY_SOURCE, machine_specs, plan};

fn names(specs: &[super::sync::Spec]) -> Vec<&str> {
    specs.iter().map(|spec| spec.name.as_str()).collect()
}

#[test]
fn machine_specs_root_each_source_under_the_claude_home() {
    let home = Path::new("/home/someone/.claude");
    let specs = machine_specs(home);
    let found: Vec<(&str, SourceKind, &Path)> = specs
        .iter()
        .map(|spec| (spec.name.as_str(), spec.kind, spec.root.as_path()))
        .collect();
    assert_eq!(
        found,
        vec![
            (
                "agents",
                SourceKind::Instructions,
                Path::new("/home/someone/.claude/agents")
            ),
            (
                "rules",
                SourceKind::Instructions,
                Path::new("/home/someone/.claude/rules")
            ),
            (
                "skills",
                SourceKind::Skills,
                Path::new("/home/someone/.claude/skills")
            ),
            (
                AGENT_MEMORY_SOURCE,
                SourceKind::Memory,
                Path::new("/home/someone/.claude/agent-memory")
            ),
        ]
    );
    assert!(specs.iter().all(|spec| !spec.declared));
}

#[test]
fn machine_specs_add_the_global_claude_file_to_the_rules_source_only() {
    let home = Path::new("/home/someone/.claude");
    for spec in machine_specs(home) {
        let expected = if spec.name == "rules" {
            vec![home.join("CLAUDE.md")]
        } else {
            Vec::new()
        };
        assert_eq!(spec.scope.extras, expected, "source {}", spec.name);
        assert!(spec.scope.includes.is_empty());
    }
}

#[tokio::test]
async fn plan_separates_the_machine_roots_that_exist_from_those_that_do_not() -> anyhow::Result<()>
{
    let home = tempfile::tempdir()?;
    std::fs::create_dir_all(home.path().join("agents"))?;
    std::fs::create_dir_all(home.path().join("agent-memory"))?;
    std::fs::write(home.path().join("skills"), "a file, not a directory")?;
    let planned = plan(home.path()).await;
    assert_eq!(names(&planned.present), vec!["agents", AGENT_MEMORY_SOURCE]);
    assert_eq!(names(&planned.missing), vec!["rules", "skills"]);
    Ok(())
}
