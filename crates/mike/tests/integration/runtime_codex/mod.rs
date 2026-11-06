mod codec_tests;
mod test_support;
mod tests_atomicity;
mod tests_errors;
mod tests_generation;
mod tests_intake;
mod tests_observations;
mod tests_reads;
mod tests_validation;

use mike::store::persistence::runtime::codex::{
    ClassificationProvenance, CodexCaptureOutcome, CodexEffectPage, CodexIntakeContext,
    CodexItemPage, CodexProjectionApplication, CodexProjectionApplyOutcome, CodexProjectionEffect,
    CodexReceiptInput, RawEventPage, StoredRuntimeReceipt,
};

mod history_control;
mod history_errors;
mod history_pages;
mod history_requests;
mod history_support;
