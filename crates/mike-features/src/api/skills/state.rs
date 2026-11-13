use serde::Serialize;
use serde_json::Value;

use crate::store::persistence::lessons::skill_review::SkillFolderRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    RetiredByYou,
    Shipped,
    RetiredUnloaded,
    RetiredCorrected,
    Removed,
    RejectedByYou,
    WaitingOnYou,
    ApprovedNotYetShipped,
    ReadyToShip,
    FailedReview,
    BeingReviewed,
    WaitingForReviewer,
    GeneratedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Ask,
}

impl Mode {
    pub fn parse(code: &str) -> Option<Self> {
        match code {
            "auto" => Some(Self::Auto),
            "ask" => Some(Self::Ask),
            _ => None,
        }
    }
}

fn installed_state(row: &SkillFolderRow) -> Option<SkillState> {
    if row.retired_by_user_at.is_some() {
        return Some(SkillState::RetiredByYou);
    }
    if row.live {
        return Some(SkillState::Shipped);
    }
    if row.review.as_deref() == Some("rejected") {
        return None;
    }
    let installs = row.installs.as_array()?;
    let measured = row
        .live_use
        .as_ref()
        .and_then(|l| l.get("retired_reason"))
        .and_then(Value::as_str);
    let reported = installs
        .iter()
        .filter_map(|i| i.get("retired_reason").and_then(Value::as_str));
    let mut reasons = reported.chain(measured);
    match reasons.find(|r| *r == "corrected_above_base" || *r == "unloaded") {
        Some("corrected_above_base") => Some(SkillState::RetiredCorrected),
        Some(_) => Some(SkillState::RetiredUnloaded),
        None => (!installs.is_empty()).then_some(SkillState::Removed),
    }
}

fn draft_state(row: &SkillFolderRow, mode: Mode) -> SkillState {
    let approved = row.approved_draft_id.is_some();
    match (row.status.as_deref(), row.review.as_deref()) {
        (None, _) => SkillState::GeneratedOnly,
        (_, Some("rejected")) => SkillState::RejectedByYou,
        (_, Some("approved")) => SkillState::ApprovedNotYetShipped,
        (Some("draft"), _) => SkillState::WaitingForReviewer,
        (Some("evaluating"), _) => SkillState::BeingReviewed,
        (Some("accepted"), _) if mode == Mode::Auto => SkillState::ReadyToShip,
        (Some("accepted"), _) => SkillState::WaitingOnYou,
        _ if approved => SkillState::ApprovedNotYetShipped,
        _ => SkillState::FailedReview,
    }
}

pub fn needs_you(row: &SkillFolderRow, mode: Mode) -> bool {
    mode == Mode::Ask
        && row.retired_by_user_at.is_none()
        && row.status.as_deref() == Some("accepted")
        && row.review.is_none()
        && row.approved_draft_id != row.draft_id
}

pub fn skill_state(row: &SkillFolderRow, mode: Mode) -> SkillState {
    if needs_you(row, mode) {
        return SkillState::WaitingOnYou;
    }
    installed_state(row).unwrap_or_else(|| draft_state(row, mode))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn row(status: Option<&str>, review: Option<&str>) -> SkillFolderRow {
        SkillFolderRow {
            folder: "ui-design".to_owned(),
            draft_id: status.map(|_| 7),
            status: status.map(str::to_owned),
            review: review.map(str::to_owned),
            revision: Some(1),
            lesson_watermark: Some(10),
            lesson_count: Some(3),
            description: None,
            drafted_at: None,
            evaluated_at: None,
            edited_at: None,
            trigger_score: None,
            patched_trigger_score: None,
            reviewer: None,
            verdict_reasons: None,
            approved_draft_id: None,
            first_written: None,
            live: false,
            installs: json!([]),
            retired_by_user_at: None,
            live_use: None,
        }
    }

    #[test]
    fn skill_retired_for_corrections_reads_as_retired_corrected() {
        let mut r = row(Some("accepted"), None);
        r.installs = json!([{ "retired_reason": "corrected_above_base" }]);
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::RetiredCorrected);
    }

    #[test]
    fn measured_retirement_counts_when_install_report_lags() {
        let mut r = row(Some("accepted"), None);
        r.installs = json!([{ "retired_reason": null }]);
        r.live_use = Some(json!({ "retired_reason": "corrected_above_base" }));
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::RetiredCorrected);
    }

    #[test]
    fn accepted_draft_in_ask_project_waits_on_you() {
        let r = row(Some("accepted"), None);
        assert_eq!(skill_state(&r, Mode::Ask), SkillState::WaitingOnYou);
        assert!(needs_you(&r, Mode::Ask));
    }

    #[test]
    fn accepted_draft_in_auto_project_is_ready_to_ship() {
        let r = row(Some("accepted"), None);
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::ReadyToShip);
        assert!(!needs_you(&r, Mode::Auto));
    }

    #[test]
    fn shipped_folder_with_newer_accepted_draft_waits_on_you() {
        let mut r = row(Some("accepted"), None);
        r.live = true;
        assert_eq!(skill_state(&r, Mode::Ask), SkillState::WaitingOnYou);
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::Shipped);
    }

    #[test]
    fn user_retirement_wins_over_live_install() {
        let mut r = row(Some("accepted"), Some("approved"));
        r.live = true;
        r.retired_by_user_at = Some(chrono::Utc::now());
        assert_eq!(skill_state(&r, Mode::Ask), SkillState::RetiredByYou);
    }

    #[test]
    fn unloaded_install_reads_as_retired_unloaded() {
        let mut r = row(Some("accepted"), Some("approved"));
        r.installs = json!([{ "retired_reason": "unloaded" }]);
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::RetiredUnloaded);
    }

    #[test]
    fn evaluator_rejection_without_approval_failed_review() {
        let r = row(Some("rejected"), None);
        assert_eq!(skill_state(&r, Mode::Ask), SkillState::FailedReview);
    }

    #[test]
    fn user_rejection_reads_as_rejected_by_you() {
        let r = row(Some("accepted"), Some("rejected"));
        assert_eq!(skill_state(&r, Mode::Ask), SkillState::RejectedByYou);
    }

    #[test]
    fn folder_removed_after_user_rejection_reads_as_rejected_by_you() {
        let mut r = row(Some("accepted"), Some("rejected"));
        r.installs = json!([{ "retired_reason": "user" }]);
        assert_eq!(skill_state(&r, Mode::Auto), SkillState::RejectedByYou);
    }

    #[test]
    fn folder_without_draft_is_generated_only() {
        assert_eq!(
            skill_state(&row(None, None), Mode::Auto),
            SkillState::GeneratedOnly
        );
    }

    #[test]
    fn unknown_mode_code_is_rejected() {
        assert_eq!(Mode::parse("sometimes"), None);
        assert_eq!(Mode::parse("ask"), Some(Mode::Ask));
    }
}
