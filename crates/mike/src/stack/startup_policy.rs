use super::protocol::RuntimeStatus;

pub(crate) fn preserve_initializing(
    last_status: Option<&RuntimeStatus>,
    child_pid: u32,
    ownership_held: bool,
) -> bool {
    ownership_held
        && last_status.is_some_and(|status| {
            status.identity.pid == child_pid && status.matches_requested && !status.ready
        })
}

#[cfg(test)]
mod tests {
    use super::super::protocol::InstanceIdentity;
    use super::*;

    #[test]
    fn matching_owned_initialization_survives_readiness_timeout_or_cooperative_stop() {
        let mut status = RuntimeStatus {
            identity: InstanceIdentity {
                instance_id: "selected".into(),
                generation: uuid::Uuid::nil(),
                pid: 42,
                started_unix_secs: 1,
            },
            ready: false,
            stopping: false,
            matches_requested: true,
            bound_listeners: None,
            persist_enabled: true,
        };
        assert!(preserve_initializing(Some(&status), 42, true));
        status.stopping = true;
        assert!(preserve_initializing(Some(&status), 42, true));
        assert!(!preserve_initializing(Some(&status), 43, true));
        assert!(!preserve_initializing(Some(&status), 42, false));
        status.ready = true;
        assert!(!preserve_initializing(Some(&status), 42, true));
        status.ready = false;
        status.matches_requested = false;
        assert!(!preserve_initializing(Some(&status), 42, true));
        assert!(!preserve_initializing(None, 42, true));
    }
}
