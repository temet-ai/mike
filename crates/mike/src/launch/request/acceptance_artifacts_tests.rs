use std::path::Path;

use super::*;
use crate::launch::fleet::acceptance_artifacts::EVIDENCE_ENV;

const RUN: &str = "fleet-example-a1234567";

fn launch(root: &Path, args: &[&str], prompt: Option<&str>) -> Result<LaunchSpec, LaunchError> {
    let options = AssembleOptions {
        program: "claude".into(),
        proxy: None,
        brief_inline_max: 800,
        briefs_dir: root,
        brief_notes: true,
        brief_by_mail: true,
    };
    let split = split_launch_args(args.iter().map(|arg| arg.to_string()))?;
    LaunchSpec::assemble(split, prompt.map(str::to_string), &options)
}

#[test]
fn initial_and_manual_acceptance_successor_share_one_evidence_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let initial = launch(
        root.path(),
        &["--run", RUN, "--seat", "acceptance"],
        Some("Judge C1"),
    )?;
    let successor = launch(
        root.path(),
        &["--run", RUN, "--seat", "acceptance-2"],
        Some("Continue judging C1"),
    )?;
    let folder = initial
        .env
        .get(EVIDENCE_ENV)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no evidence folder"))?;
    assert_eq!(successor.env.get(EVIDENCE_ENV), Some(&folder));
    let path = Path::new(&folder);
    assert!(path.is_absolute());
    assert!(path.join("comparisons").is_dir());
    assert!(path.join("artifacts").is_dir());
    assert!(!path.join("manifest.json").exists());
    for (spec, task) in [(initial, "Judge C1"), (successor, "Continue judging C1")] {
        let prompt = spec.prompt.ok_or_else(|| anyhow::anyhow!("no prompt"))?;
        assert!(prompt.starts_with(task));
        assert!(prompt.contains(folder.as_str()));
        assert!(prompt.contains(RUN));
        assert!(prompt.contains("acceptance-record"));
        assert!(prompt.contains("session notes"));
        assert!(!prompt.contains("manifest"));
    }
    Ok(())
}

#[test]
fn promptless_successor_gets_pointer_but_positional_prompt_is_preserved() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let args = ["--run", RUN, "--seat", "acceptance-2"];
    let empty = launch(root.path(), &args, None)?;
    assert!(
        empty
            .prompt
            .as_deref()
            .is_some_and(|p| p.contains("acceptance-record"))
    );
    for extra in [
        vec!["Continue the live check"],
        vec!["--verbose", "Continue the live check"],
    ] {
        let mut bare = args.to_vec();
        bare.extend(extra);
        let positional = launch(root.path(), &bare, None)?;
        assert!(positional.prompt.is_none());
        assert!(
            positional
                .args
                .iter()
                .any(|arg| arg == "Continue the live check")
        );
        assert_eq!(
            positional.env.get(EVIDENCE_ENV),
            empty.env.get(EVIDENCE_ENV)
        );
    }
    Ok(())
}

#[test]
fn only_acceptance_fleet_launches_get_an_evidence_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for args in [
        vec![],
        vec!["--role", "acceptance"],
        vec!["--run", "ordinary", "--seat", "acceptance"],
        vec!["--run", RUN, "--seat", "worker-T1"],
        vec!["--run", RUN, "--seat", "acceptance-2", "--role", "worker"],
    ] {
        let spec = launch(root.path(), &args, Some("Original brief"))?;
        assert!(!spec.env.contains_key(EVIDENCE_ENV));
        assert_eq!(spec.prompt.as_deref(), Some("Original brief"));
    }
    assert_eq!(std::fs::read_dir(root.path())?.count(), 0);
    Ok(())
}

#[test]
fn invalid_acceptance_storage_or_run_refuses_assembly() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert!(
        launch(
            root.path(),
            &["--run", "fleet-../../escape", "--role", "acceptance"],
            None
        )
        .is_err()
    );
    std::fs::write(root.path().join("fleet-runs"), "collision")?;
    assert!(launch(root.path(), &["--run", RUN, "--role", "acceptance"], None).is_err());
    Ok(())
}
