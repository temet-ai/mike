use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde_json::json;

use super::*;

const BACKEND: &str = r#"#!/bin/sh
base=${0%/*}
key="$1 $2"
printf '%s\n' "$key" >> "$base/calls"
mode=$(/bin/cat "$base/mode")
case "$key" in
    'pane process-info')
        count=$(/bin/cat "$base/probes")
        printf '%s' "$((count + 1))" > "$base/probes"
        foreground=100
        if [ "$mode" = busy ] || { [ "$mode" = delayed-prompt ] && [ "$count" -lt 2 ]; }; then foreground=200; fi
        printf '{"result":{"process_info":{"shell_pid":100,"foreground_process_group_id":%s}}}' "$foreground"
        ;;
    'pane run')
        case "$mode" in
            missing) ;;
            mismatch) destination=${4##* > }; /bin/sh -c "printf mismatch > $destination" ;;
            delayed) (/bin/sleep 0.03; /bin/sh -c "$4") >/dev/null 2>&1 & ;;
            uncertain) /bin/sh -c "$4"; printf 'uncertain' >&2; exit 1 ;;
            *) /bin/sh -c "$4" || exit 1 ;;
        esac
        printf '{"result":{}}'
        ;;
    *) printf 'unexpected fixture call' >&2; exit 1 ;;
esac
"#;

struct Fixture {
    root: tempfile::TempDir,
    cli: HerdrCli,
    args: Vec<String>,
}

impl Fixture {
    fn new(mode: &str) -> anyhow::Result<Self> {
        let root = tempfile::tempdir()?;
        let binary = root.path().join("fixture-herdr");
        std::fs::write(&binary, BACKEND)?;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700))?;
        std::fs::write(root.path().join("mode"), mode)?;
        std::fs::write(root.path().join("probes"), "0")?;
        let directory = root.path().join("guard's directory");
        std::fs::create_dir(&directory)?;
        std::fs::write(directory.join("activate"), "export MIKE_FIXTURE_ACTIVE=1\n")?;
        let settings = json!({"env": {
            "MIKE_FLEET_BROWSER_GUARD": "1",
            "MIKE_FLEET_BROWSER_GUARD_BIN": directory,
        }});
        Ok(Self {
            root,
            cli: HerdrCli::new(binary),
            args: vec!["--settings".into(), settings.to_string()],
        })
    }

    async fn run(&self) -> Result<(), LaunchError> {
        prepare_with_wait(
            &self.cli,
            &self.args,
            "w1:p1",
            Duration::from_secs(5),
            Duration::from_millis(5),
        )
        .await
    }

    fn calls(&self) -> anyhow::Result<Vec<String>> {
        Ok(std::fs::read_to_string(self.root.path().join("calls"))?
            .lines()
            .map(str::to_string)
            .collect())
    }

    fn receipt_directories(&self) -> anyhow::Result<Vec<PathBuf>> {
        Ok(
            std::fs::read_dir(self.root.path().join("guard's directory"))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect(),
        )
    }
}

pub async fn activation_waits_for_exact_receipt_and_submits_only_once() -> anyhow::Result<()> {
    for mode in ["success", "delayed", "delayed-prompt"] {
        let fixture = Fixture::new(mode)?;
        fixture.run().await?;
        let calls = fixture.calls()?;
        assert_eq!(calls.iter().filter(|call| *call == "pane run").count(), 1);
        assert_eq!(calls.last().map(String::as_str), Some("pane run"));
        if mode == "delayed-prompt" {
            assert_eq!(
                calls
                    .iter()
                    .filter(|call| *call == "pane process-info")
                    .count(),
                3
            );
        }
        assert!(fixture.receipt_directories()?.is_empty());
    }
    Ok(())
}

pub async fn missing_or_mismatched_activation_refuses_without_starting_an_agent()
-> anyhow::Result<()> {
    for mode in ["missing", "mismatch", "uncertain"] {
        let fixture = Fixture::new(mode)?;
        assert!(fixture.run().await.is_err());
        let calls = fixture.calls()?;
        assert_eq!(calls.iter().filter(|call| *call == "pane run").count(), 1);
        assert!(!calls.iter().any(|call| call == "agent start"));
        assert!(fixture.receipt_directories()?.is_empty());
    }
    Ok(())
}

pub async fn busy_prompt_times_out_without_submitting_activation() -> anyhow::Result<()> {
    let fixture = Fixture::new("busy")?;
    assert!(fixture.run().await.is_err());
    assert!(
        fixture
            .calls()?
            .iter()
            .all(|call| call == "pane process-info")
    );
    assert!(fixture.receipt_directories()?.is_empty());
    Ok(())
}

pub async fn ordinary_launches_never_call_the_backend_for_browser_setup() -> anyhow::Result<()> {
    let fixture = Fixture::new("success")?;
    prepare(&fixture.cli, &[], "w1:p1").await?;
    assert!(!fixture.root.path().join("calls").exists());
    Ok(())
}

pub fn receipt_io_failures_and_nonmatching_content_are_not_success() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    assert!(Receipt::new(&root.path().join("missing")).is_err());
    let receipt = Receipt::new(root.path())?;
    assert!(!receipt.ready()?);
    let ready = receipt.directory.path().join("ready");
    std::fs::write(&ready, "different nonce")?;
    assert!(receipt.ready().is_err());
    std::fs::remove_file(&ready)?;
    std::fs::create_dir(&ready)?;
    assert!(receipt.ready().is_err());
    Ok(())
}

pub async fn combined_helpers_share_one_activation_and_require_both() -> anyhow::Result<()> {
    for missing in [false, true] {
        let mut fixture = Fixture::new("success")?;
        let cargo = fixture.root.path().join("cargo helper");
        std::fs::create_dir(&cargo)?;
        let observed = fixture.root.path().join("helper-order");
        let browser = fixture.root.path().join("guard's directory");
        std::fs::write(
            browser.join("activate"),
            format!("printf browser >> '{}'\n", observed.display()),
        )?;
        if !missing {
            std::fs::write(
                cargo.join("activate"),
                format!("printf cargo >> '{}'\n", observed.display()),
            )?;
        }
        let mut settings: serde_json::Value = serde_json::from_str(&fixture.args[1])?;
        settings["env"]["MIKE_FLEET_CARGO_POLICY"] = json!("1");
        settings["env"]["MIKE_FLEET_CARGO_POLICY_BIN"] = json!(cargo);
        fixture.args[1] = settings.to_string();
        let result = fixture.run().await;
        assert_eq!(result.is_ok(), !missing, "{result:?}");
        assert_eq!(
            fixture
                .calls()?
                .iter()
                .filter(|call| *call == "pane run")
                .count(),
            1
        );
        assert_eq!(
            std::fs::read(&observed)?,
            if missing {
                b"browser".as_slice()
            } else {
                b"browsercargo".as_slice()
            }
        );
        assert!(fixture.receipt_directories()?.is_empty());
    }
    Ok(())
}
