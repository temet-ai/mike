use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::definition::FleetDefinition;
use super::managed_test_support::Execution;

async fn execute(
    root: &Path,
    command: &str,
    strip: &[&str],
    timeout: Option<u64>,
) -> anyhow::Result<(Option<i32>, String)> {
    let def: FleetDefinition = serde_json::from_value(json!({"execution": {
        "strip_environment": strip,
        "operations": {"verify": {"command": command, "purpose": "fixture assertions",
            "resources": [], "environment": {"FLEET_FIXTURE": "ready"}, "timeout_seconds": timeout}}
    }}))?;
    let execution = Execution::new("operation-runtime").await?;
    let mut context = execution.context();
    super::managed_command::configure(&mut context, &def, command);
    let result = super::managed_command::scope(
        context,
        super::gate_slot::run_check(
            root,
            command,
            "operation-runtime",
            (Duration::from_secs(30), "runtime fixture"),
            false,
        ),
    )
    .await;
    execution.finish().await?;
    Ok(result)
}

#[tokio::test]
async fn declared_environment_and_failure_status_reach_the_managed_runner() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (code, output) = execute(
        root.path(),
        "test \"$FLEET_FIXTURE\" = ready && printf 'observed'; exit 7",
        &[],
        None,
    )
    .await?;
    assert_eq!(code, Some(7));
    assert_eq!(output, "observed");
    Ok(())
}

#[tokio::test]
async fn a_declared_variable_is_stripped_from_the_managed_runner_and_only_that_one()
-> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let command = "printf '%s' \"${PATH:+path}:${HOME:+home}\"";
    let (_, kept) = execute(root.path(), command, &[], None).await?;
    assert_eq!(kept, "path:home");
    let (_, stripped) = execute(root.path(), command, &["HOME"], None).await?;
    assert_eq!(stripped, "path:");
    Ok(())
}

#[tokio::test]
async fn operation_timeout_stops_the_managed_process() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (code, output) = tokio::time::timeout(
        Duration::from_secs(5),
        execute(root.path(), "sleep 20; touch should-not-run", &[], Some(1)),
    )
    .await??;
    assert_eq!(code, None);
    assert!(output.contains("stopped after 1 seconds"), "{output}");
    assert!(!root.path().join("should-not-run").exists());
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed Rust, Python, Node.js and frontend TypeScript; run explicitly for cross-stack acceptance"]
async fn real_rust_typescript_python_and_no_build_operations() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let cases = [
        (
            "rust",
            "main.rs",
            "fn main() { assert_eq!(std::env::var(\"FLEET_FIXTURE\").ok().as_deref(), Some(\"ready\")); println!(\"rust passed\"); }",
            "rustc main.rs -o verify && ./verify",
        ),
        (
            "python",
            "verify.py",
            "import os\nassert os.environ['FLEET_FIXTURE'] == 'ready'\nprint('python passed')\n",
            "python3 verify.py",
        ),
        (
            "docs",
            "README.md",
            "# Project\nA task without a compiler.\n",
            "test -s README.md && grep -q '^# Project' README.md && printf 'docs passed\\n'",
        ),
    ];
    for (name, file, source, command) in cases {
        let cwd = root.path().join(name);
        std::fs::create_dir(&cwd)?;
        std::fs::write(cwd.join(file), source)?;
        let (code, output) = execute(&cwd, command, &[], None).await?;
        assert_eq!(code, Some(0), "{name}: {output}");
        assert!(output.contains(&format!("{name} passed")), "{output}");
    }
    let cwd = root.path().join("typescript");
    std::fs::create_dir(&cwd)?;
    std::fs::write(
        cwd.join("verify.ts"),
        "const total: number = 2 + 2; if (total !== 4) throw new Error('bad total'); console.log('typescript passed');\n",
    )?;
    let compiler = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/node_modules/typescript/bin/tsc")
        .canonicalize()?;
    let quoted = compiler.to_string_lossy().replace('\'', "'\\''");
    let command = format!(
        "node '{quoted}' --strict --skipLibCheck --types node --typeRoots '{}' --outDir out verify.ts && node out/verify.js",
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../frontend/node_modules/@types")
            .display()
    );
    let (code, output) = execute(&cwd, &command, &[], None).await?;
    assert_eq!(code, Some(0), "typescript: {output}");
    assert!(output.contains("typescript passed"));
    std::fs::write(
        cwd.join("verify.ts"),
        "const total: number = 'incorrect';\n",
    )?;
    let (code, output) = execute(&cwd, &command, &[], None).await?;
    assert!(code.is_some_and(|code| code != 0), "{output}");
    assert!(output.contains("not assignable"), "{output}");
    Ok(())
}
