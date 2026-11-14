use super::*;
use crate::launch::fleet::definition::TaskDefinition;
use uuid::Uuid;

fn seat(task: Option<&str>) -> SeatPlan {
    SeatPlan {
        runtime: crate::runtime::RuntimeKind::default(),
        seat: "W3".into(),
        role: "worker".into(),
        discipline: Some("rust".into()),
        model: "sonnet".into(),
        effort: None,
        agent: String::new(),
        contract: None,
        task: task.map(String::from),
        session: Uuid::nil(),
        goal: String::new(),
        instructions: String::new(),
        prompt: String::new(),
    }
}

fn def(prompt: &str) -> FleetDefinition {
    let mut def = FleetDefinition::default();
    def.tasks.insert(
        "T18".into(),
        TaskDefinition {
            system_prompt: prompt.into(),
            files: vec![".claude/fleet/fleet-command.md".into()],
            ..TaskDefinition::default()
        },
    );
    def
}

#[test]
fn a_task_system_prompt_carries_the_contract_the_authors_text_and_the_owned_files() {
    let text = text(
        &def("Check: scripts/rule-checks/fleet-text-budget.sh"),
        &seat(Some("T18")),
        Some("contract"),
    )
    .unwrap_or_default();
    assert!(text.starts_with("contract\n\n# Task T18, seat W3"));
    assert!(text.contains("fleet-text-budget.sh"));
    assert!(text.contains("Files this task owns: .claude/fleet/fleet-command.md"));
}

#[test]
fn no_task_prompt_leaves_the_seat_args_unchanged() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let args = vec![
        "--append-system-prompt-file".to_string(),
        "/c.md".to_string(),
    ];
    let def = def("");
    assert_eq!(
        apply(&def, &seat(Some("T18")), args.clone(), dir.path()),
        Ok(args.clone())
    );
    assert_eq!(apply(&def, &seat(None), args.clone(), dir.path()), Ok(args));
    Ok(())
}

#[test]
fn a_task_prompt_replaces_the_appended_file_with_one_per_seat() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let contract = dir.path().join("contract.md");
    std::fs::write(&contract, "worker contract")?;
    let args = vec![
        "--model".to_string(),
        "sonnet".to_string(),
        "--append-system-prompt-file".to_string(),
        contract.to_string_lossy().into_owned(),
    ];
    let out = apply(
        &def("Own one file."),
        &seat(Some("T18")),
        args,
        &dir.path().join("prompts"),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    let file = appended_file(&out).ok_or_else(|| anyhow::anyhow!("no appended file"))?;
    assert_eq!(
        out.iter()
            .filter(|a| *a == "--append-system-prompt-file")
            .count(),
        1
    );
    assert_eq!(&out[..2], &["--model".to_string(), "sonnet".to_string()]);
    let written = std::fs::read_to_string(file)?;
    assert!(written.starts_with("worker contract"));
    assert!(written.contains("Own one file."));
    Ok(())
}
