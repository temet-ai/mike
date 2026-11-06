use super::*;

fn identity() -> InstanceIdentity {
    InstanceIdentity {
        instance_id: "installation-a".into(),
        generation: Uuid::nil(),
        pid: 42,
        started_unix_secs: 123,
    }
}

fn state(identity: &InstanceIdentity) -> Decision<'_> {
    Decision {
        identity,
        config_file: Path::new("/daemon/config/mike/config.toml"),
        boot_fingerprint: "private-boot-digest",
        ready: true,
        stopping: false,
        persist_enabled: true,
        bound_listeners: None,
    }
}

#[test]
fn matching_instance_and_boot_are_ready_without_exposing_digest() -> Result<()> {
    let identity = identity();
    let request = Request::status(
        identity.instance_id.clone(),
        Some("private-boot-digest".into()),
    );
    let (response, stop) = state(&identity).reply(&request);
    let rendered = serde_json::to_string(&response)?;
    assert!(!rendered.contains("private-boot-digest"));
    let status = response.into_status()?;
    assert!(status.matches_requested && status.ready);
    assert!(!stop);
    Ok(())
}

#[test]
fn changed_mode_or_boot_is_never_silently_adopted() -> Result<()> {
    let identity = identity();
    let request = Request::status(
        identity.instance_id.clone(),
        Some("changed-private-digest".into()),
    );
    let (response, stop) = state(&identity).reply(&request);
    assert!(!response.into_status()?.matches_requested);
    assert!(!stop);
    Ok(())
}

#[test]
fn wrong_instance_or_stale_generation_can_never_stop_the_daemon() {
    let identity = identity();
    let mut request = Request::stop(&identity);
    request.generation = Some(Uuid::new_v4());
    let (response, stop) = state(&identity).reply(&request);
    assert_eq!(response.rejection, Some(Rejection::Generation));
    assert!(!stop);
    request.instance_id = "installation-b".into();
    let (response, stop) = state(&identity).reply(&request);
    assert_eq!(response.rejection, Some(Rejection::Instance));
    assert!(!stop);
    assert!(response.into_status().is_err());
}

#[test]
fn an_instance_rejection_names_the_daemons_config_file_and_the_commands() -> Result<()> {
    let identity = identity();
    let request = Request::status("installation-b".into(), None);
    let (response, _) = state(&identity).reply(&request);
    let wire: Response = serde_json::from_str(&serde_json::to_string(&response)?)?;
    let error = wire
        .into_status_for(Path::new("/seat/config/mike/config.toml"))
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(error.contains("/seat/config/mike/config.toml"), "{error}");
    assert!(error.contains("/daemon/config/mike/config.toml"), "{error}");
    assert!(!error.contains("not up"), "{error}");
    Ok(())
}

#[test]
fn an_older_daemon_without_a_config_file_still_gets_a_named_mismatch() -> Result<()> {
    let legacy: Response = serde_json::from_value(
        serde_json::json!({"status": null, "rejection": "instance_mismatch"}),
    )?;
    let error = legacy
        .into_status_for(Path::new("/seat/config/mike/config.toml"))
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(error.contains("different mike installation"), "{error}");
    Ok(())
}

#[test]
fn missing_generation_and_unknown_protocol_are_rejected() {
    let identity = identity();
    let mut request = Request::stop(&identity);
    request.generation = None;
    assert!(!state(&identity).reply(&request).1);
    request.protocol = 255;
    assert_eq!(
        state(&identity).reply(&request).0.rejection,
        Some(Rejection::Protocol)
    );
}

#[test]
fn explicit_stop_is_cooperative_and_repeatable_for_the_same_generation() -> Result<()> {
    let identity = identity();
    let request = Request::stop(&identity);
    let (response, stop) = state(&identity).reply(&request);
    assert!(stop && response.into_status()?.stopping);
    let mut stopping = state(&identity);
    stopping.stopping = true;
    assert!(stopping.reply(&request).1);
    Ok(())
}

#[tokio::test]
async fn protocol_round_trips_on_in_memory_streams_without_unix_sockets() -> Result<()> {
    let (mut client, mut server) = tokio::io::duplex(16_384);
    let identity = identity();
    let request = Request::status(
        identity.instance_id.clone(),
        Some("private-boot-digest".into()),
    );
    let handle = tokio::spawn(async move {
        let request: Request = read(&mut server).await?;
        let (response, _) = state(&identity).reply(&request);
        write(&mut server, &response).await
    });
    write(&mut client, &request).await?;
    let response: Response = read(&mut client).await?;
    assert!(response.into_status()?.matches_requested);
    handle.await??;
    Ok(())
}

#[tokio::test]
async fn malformed_control_messages_omit_their_contents() -> Result<()> {
    let mut malformed = &b"planted-secret"[..];
    let result: Result<Response> = read(&mut malformed).await;
    assert!(
        !result
            .err()
            .ok_or_else(|| anyhow::anyhow!("bad message"))?
            .to_string()
            .contains("planted-secret")
    );
    let bytes = vec![b'a'; 8193];
    let mut oversized = bytes.as_slice();
    let result: Result<Response> = read(&mut oversized).await;
    assert!(result.is_err());
    Ok(())
}

#[test]
fn owner_facts_require_lease_and_complete_generation_identity() -> Result<()> {
    let owner = identity();
    owner.verify_owner(Some(&owner), true)?;
    assert!(owner.verify_owner(Some(&owner), false).is_err());
    assert!(owner.verify_owner(None, true).is_err());
    let mut stale = owner.clone();
    stale.generation = Uuid::new_v4();
    assert!(owner.verify_owner(Some(&stale), true).is_err());
    stale = owner.clone();
    stale.instance_id = "other-installation".into();
    assert!(owner.verify_owner(Some(&stale), true).is_err());
    Ok(())
}

#[test]
fn actual_bound_facts_round_trip_and_legacy_status_remains_readable() -> Result<()> {
    let identity = identity();
    let addresses = ["127.0.0.1:43111".parse()?, "[::1]:43112".parse()?];
    let mut decision = state(&identity);
    decision.bound_listeners = Some(&addresses);
    let (response, _) = decision.reply(&Request::status(identity.instance_id.clone(), None));
    let value = serde_json::to_value(response)?;
    let decoded: Response = serde_json::from_value(value.clone())?;
    assert_eq!(
        decoded.into_status()?.bound_listeners.as_deref(),
        Some(addresses.as_slice())
    );
    let mut legacy = value;
    legacy["status"]
        .as_object_mut()
        .context("status object")?
        .remove("bound_listeners");
    let decoded: Response = serde_json::from_value(legacy)?;
    assert!(decoded.into_status()?.bound_listeners.is_none());
    Ok(())
}
