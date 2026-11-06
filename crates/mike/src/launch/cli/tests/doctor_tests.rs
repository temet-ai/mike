use std::path::PathBuf;

use super::{assert_key_order, fake, json, runner};
use crate::config::LaunchConfig;
use crate::launch::backend::fake::FakeBackend;
use crate::launch::cli::doctor::{self, DoctorEnv};
use crate::launch::cli::render::{CHOOSE_BACKEND, print_doctor};
use crate::launch::{BackendId, backend_for};

fn env(ca: PathBuf) -> DoctorEnv {
    DoctorEnv {
        sidecar_url: "http://127.0.0.1:1".into(),
        inspect_addr: "127.0.0.1:1".into(),
        ca_candidates: vec![ca],
        claude_bin: "sh".into(),
    }
}

fn check<'a>(report: &'a doctor::DoctorReport, name: &str) -> &'a crate::launch::Check {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name} in {:?}", report.checks))
}

#[tokio::test]
async fn doctor_available_backend_runs_every_check_and_names_failures() {
    let home = tempfile::tempdir().unwrap();
    let ca = home.path().join("ca.pem");
    std::fs::write(&ca, "pem").unwrap();
    let backend = fake();
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = doctor::execute(&runner, &env(ca)).await;
    assert_eq!(report.backend, BackendId::Herdr);
    assert!(check(&report, "backend chosen").passed);
    assert!(check(&report, "probe: fake").passed);
    assert!(check(&report, "validate: fake").passed);
    assert!(check(&report, "ca cert").passed);
    assert!(check(&report, "claude").passed);
    let sidecar = check(&report, "sidecar");
    assert!(!sidecar.passed);
    assert!(sidecar.remedy.as_deref().unwrap().contains("proxy-forward"));
    assert!(!check(&report, "inspect api").passed);
    assert!(!check(&report, "database").passed);
    assert!(!check(&report, "last validation").passed);
    assert!(!report.passed);
    assert_eq!(print_doctor(&report, true), 1);
    let line = report.to_json_line();
    assert_key_order(&line, &["backend", "passed", "checks"]);
    let value = json(&line);
    assert_eq!(
        value["checks"].as_array().unwrap().len(),
        report.checks.len()
    );
    let first = &value["checks"][0];
    for key in ["name", "passed", "detail", "remedy"] {
        assert!(first.get(key).is_some(), "{key} missing from {first}");
    }
}

#[tokio::test]
async fn doctor_unavailable_backend_skips_validation_with_a_remedy() {
    let home = tempfile::tempdir().unwrap();
    let backend = FakeBackend::default();
    let config = LaunchConfig::default();
    let runner = runner(&backend, &config, home.path());
    let report = doctor::execute(&runner, &env(home.path().join("missing.pem"))).await;
    assert!(!check(&report, "probe: fake").passed);
    let validate = check(&report, "validate");
    assert!(!validate.passed);
    assert!(validate.detail.contains("skipped"));
    assert!(!check(&report, "ca cert").passed);
    assert!(!report.passed);
}

#[tokio::test]
async fn doctor_none_backend_fails_backend_chosen_with_the_setup_remedy() {
    let home = tempfile::tempdir().unwrap();
    let none = backend_for(BackendId::None).unwrap();
    let config = LaunchConfig::default();
    let runner = runner(none.as_ref(), &config, home.path());
    let report = doctor::execute(&runner, &env(home.path().join("missing.pem"))).await;
    let chosen = check(&report, "backend chosen");
    assert!(!chosen.passed);
    assert_eq!(chosen.remedy.as_deref(), Some(CHOOSE_BACKEND));
    assert_eq!(print_doctor(&report, false), 1);
}
