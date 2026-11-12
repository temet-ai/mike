use serde_json::json;

use crate::test_support::options;

#[test]
fn caller_policy_and_server_name_replace_mike_defaults() {
    let mut options = options();
    options.policy.sandbox = crate::SandboxMode::ReadOnly;
    options.policy.approval = crate::ApprovalPolicy::OnRequest;
    if let Some(mcp) = &mut options.mcp {
        mcp.name = "custom-tools".into();
    }
    let params = options.thread_start();
    assert_eq!(params["sandbox"], "read-only");
    assert_eq!(params["approvalPolicy"], "on-request");
    assert!(params["config"]["mcp_servers"]["custom-tools"].is_object());
    assert!(params["config"]["mcp_servers"].get("mike").is_none());
}

#[test]
fn model_catalogue_matches_supported_identity_fields_and_unknowns() {
    for key in ["model", "id", "slug"] {
        let models = json!({"data": [{key: "fixture-model"}]});
        assert!(crate::model::listed(&models, "fixture-model"));
        assert!(!crate::model::listed(&models, "missing-model"));
    }
    assert!(!crate::model::listed(&json!({"data": []}), "fixture-model"));
    assert!(!crate::model::listed(&json!({}), "fixture-model"));
}

#[test]
fn thread_start_params_carry_seat_policy_and_mike_binding_without_claude_flags() {
    let params = options().thread_start();
    assert_eq!(params["sandbox"], "workspace-write");
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["ephemeral"], false);
    assert_eq!(params["model"], "gpt-6.1-sol");
    assert_eq!(params["developerInstructions"], "contract");
    let mike = &params["config"]["mcp_servers"]["mike"];
    assert_eq!(mike["command"], "mike-mcp");
    assert_eq!(mike["env"]["CLAUDE_CODE_SESSION_ID"], "s-1");
    assert_eq!(mike["env"]["MIKE_MCP_PROFILE"], "worker");
    let text = params.to_string();
    for flag in [
        "--model",
        "--agent",
        "--settings",
        "--append-system-prompt",
        "--effort",
    ] {
        assert!(!text.contains(flag), "{flag} leaked into {text}");
    }
}

#[test]
fn resume_fork_and_turn_params_name_the_thread() {
    let seat = options();
    assert_eq!(seat.thread_resume("t-1")["threadId"], "t-1");
    assert_eq!(seat.thread_fork("t-1")["threadId"], "t-1");
    assert_eq!(seat.thread_fork("t-1")["ephemeral"], false);
    let turn = seat.turn_start("t-1", "brief");
    assert_eq!(turn["input"][0]["text"], "brief");
    assert_eq!(turn["effort"], "high");
}
