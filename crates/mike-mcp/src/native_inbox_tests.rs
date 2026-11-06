use super::*;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

const SENDER: &str = "2408284e-fa87-4d71-baf3-4297423595c3";

fn message(body: &str) -> ClaimedChannelMessage {
    ClaimedChannelMessage {
        message_uuid: "11111111-2222-3333-4444-555555555555".to_string(),
        from_session_uuid: Some(SENDER.to_string()),
        from_display_name: Some("worker-T3-2408284e".to_string()),
        from_role: Some("fleet-worker".to_string()),
        envelope: "<system-reminder>old</system-reminder>".to_string(),
        body: body.to_string(),
        from_peer_address: Some("uds:/run/user/1000/cc-socks/42.sock".to_string()),
    }
}

#[test]
fn envelope_carries_address_session_and_name_in_claude_codes_order() {
    assert_eq!(
        envelope(&message("Schema migration finished\nRebase is safe now.")),
        "<cross-session-message from=\"uds:/run/user/1000/cc-socks/42.sock\" \
         from-session=\"2408284e-fa87-4d71-baf3-4297423595c3\" from-name=\"worker-T3-2408284e\">\n\
         Schema migration finished\nRebase is safe now.\n</cross-session-message>"
    );
}

#[test]
fn envelope_without_a_live_sender_address_leaves_from_out() {
    let mut m = message("hi");
    m.from_peer_address = None;
    let text = envelope(&m);
    assert!(text.starts_with("<cross-session-message from-session=\""));
    assert!(!text.contains(" from=\""));
}

#[test]
fn mail_with_no_sender_is_named_mike() {
    let mut m = message("hi");
    m.from_session_uuid = None;
    m.from_display_name = None;
    m.from_role = None;
    m.from_peer_address = None;
    assert_eq!(
        envelope(&m),
        "<cross-session-message from-name=\"mike\">\nhi\n</cross-session-message>"
    );
}

#[test]
fn sender_without_a_name_falls_back_to_role_then_session_prefix() {
    let mut m = message("hi");
    m.from_display_name = None;
    assert!(envelope(&m).contains("from-name=\"fleet-worker\""));
    m.from_role = Some("  ".to_string());
    assert!(envelope(&m).contains("from-name=\"2408284e\""));
}

#[test]
fn quotes_and_angle_brackets_are_stripped_from_the_name() {
    let mut m = message("hi");
    m.from_display_name = Some("a\"b<c>d\n".to_string());
    assert!(envelope(&m).contains("from-name=\"abcd\""));
}

#[test]
fn an_unshaped_address_or_session_is_left_out() {
    let mut m = message("hi");
    m.from_peer_address = Some("uds:/tmp/a\" b.sock".to_string());
    m.from_session_uuid = Some("not-a-uuid".to_string());
    let text = envelope(&m);
    assert!(!text.contains(" from=\""));
    assert!(!text.contains("from-session"));
}

#[test]
fn socket_line_is_a_user_message_with_the_mail_id() {
    let line = socket_line(&message("hi"));
    assert_eq!(line["type"], "user");
    assert_eq!(line["priority"], "next");
    assert_eq!(line["message"]["role"], "user");
    assert_eq!(line["msg_id"], "11111111-2222-3333-4444-555555555555");
    assert_eq!(line["from"], "uds:/run/user/1000/cc-socks/42.sock");
}

#[test]
fn inbox_reads_a_path_or_a_uds_address_and_needs_an_absolute_path() {
    let inbox = NativeInbox::from_values(Some("/run/x.sock".into()), Some(" tok ".into()));
    assert_eq!(
        inbox,
        Some(NativeInbox {
            socket: PathBuf::from("/run/x.sock"),
            token: Some("tok".into())
        })
    );
    let prefixed = NativeInbox::from_values(Some("uds:/run/x.sock".into()), None);
    assert_eq!(
        prefixed.map(|i| i.peer_address()),
        Some("uds:/run/x.sock".into())
    );
    assert_eq!(NativeInbox::from_values(Some("x.sock".into()), None), None);
    assert_eq!(NativeInbox::from_values(None, Some("tok".into())), None);
}

#[test]
fn delivery_with_neither_route_claims_nothing() {
    let none = Delivery {
        inbox: None,
        channel: false,
    };
    assert!(!none.any());
    assert_eq!(none.peer_address(), None);
}

#[tokio::test]
async fn deliver_writes_the_auth_line_then_the_message_line() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!("mike-native-inbox-{}.sock", uuid_suffix()));
    let listener = UnixListener::bind(&path)?;
    let inbox = NativeInbox {
        socket: path.clone(),
        token: Some("secret".into()),
    };
    let reader = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        let mut received = String::new();
        stream.read_to_string(&mut received).await?;
        Ok::<String, std::io::Error>(received)
    });
    inbox.deliver(&message("hi")).await?;
    let received = reader.await??;
    let lines: Vec<Value> = received
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], json!({ "type": "auth", "token": "secret" }));
    assert_eq!(lines[1]["type"], "user");
    std::fs::remove_file(&path)?;
    Ok(())
}

fn uuid_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

#[tokio::test]
async fn deliver_to_a_missing_socket_is_an_error() {
    let inbox = NativeInbox {
        socket: PathBuf::from("/nonexistent/mike-native-inbox.sock"),
        token: None,
    };
    assert!(inbox.deliver(&message("hi")).await.is_err());
}
