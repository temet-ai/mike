use serde::Serialize;
use uuid::Uuid;

use super::backend::BackendId;
use super::brief::BriefMode;
use super::error::LaunchError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelCheck {
    Found,
    Timeout,
    DbUnreachable,
    NoSessionId,
    Disabled,
}

impl ChannelCheck {
    pub fn failed(self) -> bool {
        matches!(self, ChannelCheck::Timeout)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchOutput {
    pub backend: BackendId,
    pub workspace: Option<String>,
    pub folder: String,
    pub surface: Option<String>,
    pub session: Option<Uuid>,
    pub reused: bool,
    pub adopted: bool,
    pub opened: bool,
    pub delivered: Option<bool>,
    pub confirmed: bool,
    pub channel_check: ChannelCheck,
    pub brief_mode: Option<BriefMode>,
    pub run: Option<String>,
    pub seat: Option<String>,
    pub reason: Option<String>,
}

impl LaunchOutput {
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            format!(
                "{{\"backend\":\"{}\",\"reason\":\"json encoding failed: {e}\"}}",
                self.backend
            )
        })
    }

    pub fn exit_code(&self) -> i32 {
        if self.reason.is_some() || self.channel_check.failed() {
            return 1;
        }
        if self.opened && self.delivered == Some(true) && !self.confirmed {
            return 2;
        }
        0
    }
}

pub fn exit_code_for(result: &Result<LaunchOutput, LaunchError>) -> i32 {
    match result {
        Ok(output) => output.exit_code(),
        Err(err) => err.exit_code(),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
pub mod tests {
    use super::*;

    fn success() -> LaunchOutput {
        LaunchOutput {
            backend: BackendId::Herdr,
            workspace: Some("w1".into()),
            folder: "/abs/folder".into(),
            surface: Some("w1:p2".into()),
            session: Some(Uuid::nil()),
            reused: true,
            adopted: false,
            opened: true,
            delivered: Some(true),
            confirmed: true,
            channel_check: ChannelCheck::Found,
            brief_mode: Some(BriefMode::Inline),
            run: None,
            seat: Some("obs".into()),
            reason: None,
        }
    }

    #[test]
    fn json_line_has_exactly_the_contract_keys_in_order() {
        let line = success().to_json_line();
        assert!(!line.contains('\n'));
        let expected = [
            "backend",
            "workspace",
            "folder",
            "surface",
            "session",
            "reused",
            "adopted",
            "opened",
            "delivered",
            "confirmed",
            "channel_check",
            "brief_mode",
            "run",
            "seat",
            "reason",
        ];
        let positions: Vec<usize> = expected
            .iter()
            .map(|k| line.find(&format!("\"{k}\":")).unwrap())
            .collect();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "key order in {line}"
        );
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value.as_object().unwrap().len(), expected.len());
        assert_eq!(value["backend"], "herdr");
        assert_eq!(value["channel_check"], "found");
        assert_eq!(value["brief_mode"], "inline");
        assert_eq!(value["run"], serde_json::Value::Null);
        assert_eq!(value["reason"], serde_json::Value::Null);
        assert_eq!(value["session"], "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn exit_code_success_zero_reason_one_timeout_one_unconfirmed_two() {
        assert_eq!(success().exit_code(), 0);
        let mut failed = success();
        failed.reason = Some("backend_unavailable".into());
        assert_eq!(failed.exit_code(), 1);
        let mut timed_out = success();
        timed_out.channel_check = ChannelCheck::Timeout;
        assert_eq!(timed_out.exit_code(), 1);
        let mut unconfirmed = success();
        unconfirmed.confirmed = false;
        assert_eq!(unconfirmed.exit_code(), 2);
        let mut no_prompt = success();
        no_prompt.delivered = None;
        no_prompt.confirmed = false;
        assert_eq!(no_prompt.exit_code(), 0);
    }

    #[test]
    fn channel_check_serialises_snake_case_and_only_timeout_fails() {
        let all = [
            (ChannelCheck::Found, "found", false),
            (ChannelCheck::Timeout, "timeout", true),
            (ChannelCheck::DbUnreachable, "db_unreachable", false),
            (ChannelCheck::NoSessionId, "no_session_id", false),
            (ChannelCheck::Disabled, "disabled", false),
        ];
        for (value, text, fails) in all {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format!("\"{text}\"")
            );
            assert_eq!(value.failed(), fails);
        }
    }

    #[test]
    fn exit_code_for_maps_both_sides() {
        assert_eq!(exit_code_for(&Ok(success())), 0);
        assert_eq!(exit_code_for(&Err(LaunchError::BackendNone)), 69);
        assert_eq!(
            exit_code_for(&Err(LaunchError::ExecutableNotFound {
                name: "herdr".into(),
                remedy: "r".into()
            })),
            127
        );
    }
}
