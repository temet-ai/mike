use super::*;

fn request(program: &str, args: &[String], session: Option<&str>) -> OpenRequest {
    let mut env = BTreeMap::new();
    if let Some(session) = session {
        env.insert("MIKE_SESSION_ID".into(), session.into());
    }
    OpenRequest {
        folder: "/tmp".into(),
        placement: Placement::Tab,
        env,
        title: program.into(),
        run_tab: None,
        run: None,
        program: program.into(),
        args: args.into(),
        name: "fixture".into(),
        prompt: None,
        prompt_wait_ms: 0,
    }
}

#[test]
fn claude_session_identity_keeps_existing_space_equals_and_resume_behavior() {
    let id = Uuid::new_v4();
    for args in [
        vec!["--session-id".into(), id.to_string()],
        vec![format!("--session-id={id}")],
    ] {
        assert_eq!(request("claude", &args, None).session_uuid(), Some(id));
    }
    assert_eq!(
        request("claude", &["--resume".into()], None).session_uuid(),
        None
    );
    assert_eq!(
        request("claude", &["--resume".into(), id.to_string()], None).session_uuid(),
        Some(id)
    );
    assert_eq!(
        request(
            "claude",
            &["--resume".into(), id.to_string(), "--fork-session".into()],
            None
        )
        .session_uuid(),
        None
    );
    assert_eq!(
        request("claude", &["--continue".into()], None).session_uuid(),
        None
    );
}

#[test]
fn codex_identity_uses_the_assigned_mike_uuid_without_interpreting_prompt_tokens() {
    let id = Uuid::new_v4();
    let misleading = vec!["--session-id".into(), Uuid::new_v4().to_string()];
    assert_eq!(
        request("codex", &misleading, Some(&id.to_string())).session_uuid(),
        Some(id)
    );
    assert_eq!(
        request("codex", &misleading, Some("invalid")).session_uuid(),
        None
    );
    assert_eq!(request("codex", &misleading, None).session_uuid(), None);
}
