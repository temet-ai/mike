use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

pub(super) const LOGGED_IN_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '2.1.280 (Claude Code)\n'
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  printf '{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"dawid@example.com","orgId":"org-1","orgName":"Org","subscriptionType":"max"}\n'
  exit 0
fi
exit 1
"#;

pub(super) const LOGGED_OUT_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '2.1.280 (Claude Code)\n'
  exit 0
fi
if [ "$1" = "auth" ] && [ "$2" = "status" ]; then
  printf '{"loggedIn":false}\n'
  exit 0
fi
exit 1
"#;

pub(super) const LOGIN_URL: &str = "https://claude.com/oauth/authorize?code=abc";

pub(super) const LOGIN_SUCCEEDS_ON_ANY_CODE_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
  echo "Opening browser to sign in..."
  echo "If the browser didn't open, visit: https://claude.com/oauth/authorize?code=abc"
  read -r pasted_code
  if [ -n "$pasted_code" ]; then
    exit 0
  fi
  exit 1
fi
exit 1
"#;

pub(super) const LOGIN_FAILS_ON_ANY_CODE_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
  echo "If the browser didn't open, visit: https://claude.com/oauth/authorize?code=abc"
  read -r pasted_code
  exit 1
fi
exit 1
"#;

pub(super) const LOGIN_HANGS_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "login" ]; then
  echo "If the browser didn't open, visit: https://claude.com/oauth/authorize?code=hang"
  sleep 30
  exit 0
fi
exit 1
"#;

pub(super) const LOGOUT_OK_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "logout" ]; then
  exit 0
fi
exit 1
"#;

pub(super) const LOGOUT_FAILS_SCRIPT: &str = r#"#!/bin/sh
if [ "$1" = "auth" ] && [ "$2" = "logout" ]; then
  echo "not logged in" >&2
  exit 1
fi
exit 1
"#;

pub(super) struct FakeCli {
    dir: tempfile::TempDir,
}

impl FakeCli {
    pub(super) fn new(script: &str) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        let bin = dir.path().join("claude");
        std::fs::write(&bin, script)?;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))?;
        Ok(Self { dir })
    }

    pub(super) fn bin(&self) -> PathBuf {
        self.dir.path().join("claude")
    }
}
