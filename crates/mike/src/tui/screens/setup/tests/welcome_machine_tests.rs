use super::super::harness::{Harness, render_step, words};
use super::super::step::WizardStep;
use super::super::steps::machine::Machine;
use super::super::steps::welcome::Welcome;

#[test]
fn the_welcome_step_asks_before_looking_and_says_what_it_looks_at() {
    let step = Welcome::new();
    let text = render_step(&step);
    assert!(
        text.contains("Look at this machine (recommended)"),
        "{text}"
    );
    assert!(text.contains("Do not look"), "{text}");
    assert!(text.contains("with their versions"), "{text}");
    let flowed = text
        .replace('│', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(flowed.contains("`mike setup <step>`"), "{text}");
}

#[tokio::test]
async fn the_machine_step_lists_the_tools_it_found_and_names_the_rest() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.add_executable(
        "docker",
        "#!/bin/sh\necho 'Docker version 27.3.1, build abc'\n",
    )?;
    harness.add_executable("tmux", "#!/bin/sh\necho 'tmux 3.5a'\n")?;
    let mut step = Machine::new();
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    assert!(text.contains("Docker:"), "{text}");
    assert!(text.contains("tmux:"), "{text}");
    assert!(text.contains("Not found:"), "{text}");
    assert!(
        text.contains("Homebrew"),
        "the missing ones are named: {text}"
    );
    let found_section = text.split("Not found").next().unwrap_or_default();
    assert!(!found_section.contains("Homebrew"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_machine_step_uses_found_and_not_found_only() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    harness.add_executable("docker", "#!/bin/sh\n")?;
    let mut step = Machine::new();
    step.enter(&mut harness.cx()).await;
    let text = render_step(&step);
    let vocabulary = words(&text);
    for banned in [
        "installed",
        "present",
        "available",
        "ok",
        "up",
        "down",
        "active",
        "ready",
    ] {
        assert!(
            !vocabulary.contains(banned),
            "banned word {banned:?}: {text}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn looking_again_picks_up_a_tool_added_since() -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let mut step = Machine::new();
    step.enter(&mut harness.cx()).await;
    assert!(!render_step(&step).contains("ollama:"));
    harness.add_executable("ollama", "#!/bin/sh\necho 'ollama version is 0.9.0'\n")?;
    step.key(
        super::super::harness::key(crossterm::event::KeyCode::Char('r')),
        &mut harness.cx(),
    )
    .await;
    assert!(render_step(&step).contains("Ollama:"));
    Ok(())
}
