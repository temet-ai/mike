use crate::doctor::{Report, Status};
use crate::finding::Finding;

pub(super) fn findings_from(report: &Report) -> Vec<Finding> {
    report
        .checks
        .iter()
        .map(|check| {
            let text = match (&check.fix, check.status) {
                (Some(fix), Status::Warn | Status::Fail) => {
                    format!(
                        "{}: {}. Fix: `{fix}`.",
                        check.name,
                        check.detail.trim_end_matches('.')
                    )
                }
                _ => format!("{}: {}", check.name, check.detail),
            };
            match check.status {
                Status::Ok => Finding::ok(text),
                Status::Warn => Finding::note(text),
                Status::Fail => Finding::failed(text),
            }
        })
        .collect()
}
