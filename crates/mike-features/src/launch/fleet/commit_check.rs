use std::path::Path;
use std::time::Duration;

use super::definition::{CommitCheckDefinition, FleetDefinition};

const CHECK_TIMEOUT: Duration = Duration::from_secs(120);

pub fn files_for(check: &CommitCheckDefinition, repo: &str, paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| check.files.is_empty() || check.files.iter().any(|end| path.ends_with(end)))
        .filter(|path| Path::new(repo).join(path).is_file())
        .cloned()
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub warnings: Vec<String>,
    pub passed: Vec<String>,
}

pub async fn run(def: &FleetDefinition, paths: &[String]) -> Outcome {
    let mut outcome = Outcome::default();
    for check in def.commit_check.declared() {
        let files = files_for(check, &def.repo, paths);
        if files.is_empty() {
            continue;
        }
        match one(&def.repo, check.command.trim(), &files).await {
            None => outcome.passed.push(check.command.trim().to_string()),
            Some(warning) => outcome.warnings.push(warning),
        }
    }
    outcome
}

async fn one(repo: &str, command: &str, files: &[String]) -> Option<String> {
    let child = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{command} \"$@\" 2>&1"))
        .arg("mike")
        .args(files)
        .current_dir(repo)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(CHECK_TIMEOUT, child).await {
        Ok(Ok(output)) if output.status.success() => None,
        Ok(Ok(output)) => Some(format!(
            "`{command}` does not pass on the files just committed:\n{}",
            String::from_utf8_lossy(&output.stdout).trim_end()
        )),
        Ok(Err(err)) => Some(format!(
            "`{command}` could not run on the committed files: {err}"
        )),
        Err(_) => Some(format!(
            "`{command}` did not finish on the committed files within {} seconds",
            CHECK_TIMEOUT.as_secs()
        )),
    }
}

pub fn describe_passed(passed: &[String]) -> Option<String> {
    (!passed.is_empty()).then(|| {
        let listed: Vec<String> = passed.iter().map(|c| format!("`{c}`")).collect();
        format!(
            "commit checks passed on the committed files: {}",
            listed.join(", ")
        )
    })
}

#[cfg(test)]
pub mod tests {
    use super::super::definition::CommitChecks;
    use super::*;

    fn def(repo: &str, command: &str) -> FleetDefinition {
        FleetDefinition {
            repo: repo.to_string(),
            commit_check: CommitChecks(vec![CommitCheckDefinition {
                command: command.to_string(),
                files: vec![".txt".to_string()],
            }]),
            ..FleetDefinition::default()
        }
    }

    #[tokio::test]
    async fn a_commit_check_that_fails_on_a_committed_file_comes_back_as_a_warning()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().to_string_lossy().into_owned();
        std::fs::write(dir.path().join("good.txt"), "tidy\n")?;
        std::fs::write(dir.path().join("bad name.txt"), "untidy  \n")?;
        std::fs::write(dir.path().join("other.md"), "untidy  \n")?;
        let failing = def(&repo, "! grep -l '  $'");
        let paths = vec![
            "good.txt".to_string(),
            "bad name.txt".to_string(),
            "other.md".to_string(),
            "gone.txt".to_string(),
        ];
        assert_eq!(
            files_for(&failing.commit_check.0[0], &repo, &paths),
            vec!["good.txt".to_string(), "bad name.txt".to_string()],
            "only files that exist and end as declared are checked"
        );
        let warned = run(&failing, &paths)
            .await
            .warnings
            .pop()
            .unwrap_or_default();
        assert!(
            warned.starts_with("`! grep -l '  $'` does not pass on the files just committed:\n"),
            "{warned}"
        );
        assert!(warned.ends_with("bad name.txt"), "{warned}");
        let clean = run(&failing, &["good.txt".to_string()]).await;
        assert!(clean.warnings.is_empty());
        assert_eq!(clean.passed, vec!["! grep -l '  $'".to_string()]);
        Ok(())
    }

    #[tokio::test]
    async fn no_declared_commit_check_means_nothing_runs() {
        let quiet = def("/nonexistent", "");
        assert_eq!(
            run(&quiet, &["a.txt".to_string()]).await,
            Outcome::default()
        );
    }

    #[tokio::test]
    async fn two_commit_checks_each_run_once_on_the_commits_own_files() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().to_string_lossy().into_owned();
        for n in 0..10 {
            std::fs::write(dir.path().join(format!("peer{n}.txt")), "untidy  \n")?;
        }
        std::fs::write(dir.path().join("a.txt"), "tidy\n")?;
        std::fs::write(dir.path().join("b.txt"), "tidy\n")?;
        let both = FleetDefinition {
            repo,
            commit_check: CommitChecks(vec![
                CommitCheckDefinition {
                    command: "! grep -l '  $'".into(),
                    files: vec![".txt".into()],
                },
                CommitCheckDefinition {
                    command: "test $# -le 2 ||  exit 3; true".into(),
                    files: vec![".txt".into()],
                },
            ]),
            ..FleetDefinition::default()
        };
        let own = vec!["a.txt".to_string(), "b.txt".to_string()];
        let outcome = run(&both, &own).await;
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
        assert_eq!(outcome.passed.len(), 2);
        let said = describe_passed(&outcome.passed).unwrap_or_default();
        assert!(
            said.contains("! grep -l") && said.contains("test $# -le 2"),
            "{said}"
        );
        Ok(())
    }

    #[test]
    fn one_commit_check_object_and_a_list_both_parse() -> anyhow::Result<()> {
        let one: FleetDefinition = serde_json::from_value(serde_json::json!(
            {"commit_check": {"command": "fmt", "files": [".rs"]}}
        ))?;
        let many: FleetDefinition = serde_json::from_value(serde_json::json!(
            {"commit_check": [{"command": "fmt", "files": [".rs"]}, {"command": "len"}]}
        ))?;
        assert_eq!(one.commit_check.0.len(), 1);
        assert_eq!(many.commit_check.0.len(), 2);
        assert_eq!(FleetDefinition::default().commit_check.declared().len(), 0);
        Ok(())
    }
}
