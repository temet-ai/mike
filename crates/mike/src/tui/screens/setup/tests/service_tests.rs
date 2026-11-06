use super::super::steps::service::{ServiceAction, ServiceInputs, plan_panel};
use crate::service::Observation;

fn observation(installed: bool, running: bool, dev: Option<&str>) -> Observation {
    Observation {
        manager_name: "launchd",
        unit_lines: vec!["proxy: not installed".to_string()],
        installed,
        running,
        dev_stack: dev.map(str::to_string),
    }
}

fn inputs(observation: Option<Observation>, linux: bool) -> ServiceInputs {
    ServiceInputs {
        observation,
        proxy_answers: false,
        linux,
    }
}

fn actions(inputs: &ServiceInputs) -> Vec<ServiceAction> {
    plan_panel(inputs)
        .choices
        .iter()
        .map(|c| c.action)
        .collect()
}

#[test]
fn a_machine_without_a_service_manager_has_nothing_to_choose() {
    let panel = plan_panel(&inputs(None, false));
    assert!(panel.choices.is_empty());
    let note = panel.note.unwrap_or_default();
    assert!(
        note.contains("macOS (launchd) and Linux (systemd)"),
        "{note}"
    );
}

#[test]
fn a_running_developer_stack_skips_the_step_with_a_plain_note() {
    let panel = plan_panel(&inputs(
        Some(observation(false, false, Some("mike-stack-proxy.service"))),
        true,
    ));
    assert!(panel.choices.is_empty());
    let note = panel.note.unwrap_or_default();
    assert!(
        note.contains("The developer stack is running here (mike-stack-proxy.service)"),
        "{note}"
    );
    assert!(note.contains("this step does nothing"), "{note}");
}

#[test]
fn an_uninstalled_service_recommends_the_install_and_shows_the_command() {
    let panel = plan_panel(&inputs(Some(observation(false, false, None)), false));
    assert_eq!(
        panel.choices.iter().map(|c| c.action).collect::<Vec<_>>(),
        vec![ServiceAction::Install, ServiceAction::Leave]
    );
    let install = &panel.choices[0];
    assert!(install.recommended);
    assert_eq!(install.commands, vec!["mike service install"]);
    assert!(
        install
            .why
            .contains("ai.temet.mike.proxy and ai.temet.mike.forward"),
        "{}",
        install.why
    );
    assert!(install.root_commands.is_empty(), "launchd needs no root");
}

#[test]
fn on_linux_the_linger_command_is_printed_for_the_user_to_run() {
    let panel = plan_panel(&inputs(Some(observation(false, false, None)), true));
    let install = &panel.choices[0];
    assert_eq!(
        install.root_commands,
        vec!["sudo loginctl enable-linger $USER"]
    );
    assert!(
        install
            .why
            .contains("mike.service and mike-forward.service")
    );
}

#[test]
fn an_installed_service_recommends_a_check_and_offers_a_restart() {
    let list = actions(&inputs(Some(observation(true, true, None)), false));
    assert_eq!(
        list,
        vec![
            ServiceAction::Check,
            ServiceAction::Restart,
            ServiceAction::Leave
        ]
    );
    let panel = plan_panel(&inputs(Some(observation(true, true, None)), false));
    assert_eq!(
        panel.selected().map(|c| c.action),
        Some(ServiceAction::Check)
    );
}

#[test]
fn the_found_lines_name_the_manager_the_units_and_whether_a_proxy_answers() {
    let mut with = inputs(Some(observation(false, false, None)), false);
    with.proxy_answers = true;
    let found = plan_panel(&with).found.join("\n");
    assert!(found.contains("Service manager: launchd"), "{found}");
    assert!(found.contains("proxy: not installed"), "{found}");
    assert!(
        found.contains("A mike proxy answers on the inspect address"),
        "{found}"
    );
}
