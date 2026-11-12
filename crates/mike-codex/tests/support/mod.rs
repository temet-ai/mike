pub mod fake;

use std::os::unix::fs::PermissionsExt;

use mike_codex::{ApprovalPolicy, SandboxMode, SeatOptions, ThreadPolicy};

pub struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> anyhow::Result<Self> {
        let root = tempfile::tempdir()?;
        let executable = std::env::current_exe()?;
        let executable = executable.to_string_lossy().replace('\'', "'\\''");
        let program = root.path().join("fake-codex");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\n[ \"$1\" = app-server ] || exit 64\nexec '{executable}' --exact fake_app_server --ignored --nocapture\n"
            ),
        )?;
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o700))?;
        Ok(Self { root })
    }

    pub fn options(&self) -> SeatOptions {
        SeatOptions {
            program: self.root.path().join("fake-codex"),
            cwd: self.root.path().to_path_buf(),
            codex_home: self.root.path().join("home"),
            model: Some("fixture-model".into()),
            effort: Some("high".into()),
            developer_instructions: "fixture contract".into(),
            policy: ThreadPolicy {
                sandbox: SandboxMode::WorkspaceWrite,
                approval: ApprovalPolicy::Never,
            },
            mcp: None,
        }
    }
}
