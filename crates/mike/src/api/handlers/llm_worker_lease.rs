use super::*;
use crate::store::persistence::worker_leases::WorkerLeaseRow;
use uuid::Uuid;

fn row(state: CapabilityState, detail: String, fix: String) -> CapabilityRow {
    CapabilityRow {
        id: "llm_worker_lease",
        label: "LLM worker ownership",
        state,
        detail,
        fix,
    }
}

fn held_elsewhere(lease: &WorkerLeaseRow) -> String {
    format!(
        "{} (machine {}, last heartbeat {:.0} s ago)",
        lease.role, lease.machine_uuid, lease.age_secs
    )
}

fn listed(items: &[String]) -> String {
    if items.is_empty() {
        "no role".to_owned()
    } else {
        items.join(", ")
    }
}

pub(crate) fn llm_worker_lease_row(
    machine: Option<Uuid>,
    leases: &[WorkerLeaseRow],
) -> CapabilityRow {
    let Some(machine) = machine else {
        return row(
            CapabilityState::Degraded,
            "This machine has no identity file, so it cannot claim LLM worker roles and runs every one of them without a lease. A second proxy on the same database would run them as well.".to_owned(),
            "The proxy creates machine.json in the mike state directory ($XDG_STATE_HOME/mike or ~/.local/state/mike) when it is missing; check that the directory is writable and see the proxy log.".to_owned(),
        );
    };
    if leases.is_empty() {
        return row(
            CapabilityState::Active,
            format!("Machine {machine}: no LLM worker role has been claimed in this database yet"),
            String::new(),
        );
    }
    let (own, other): (Vec<_>, Vec<_>) = leases
        .iter()
        .partition(|lease| lease.machine_uuid == machine);
    let own: Vec<String> = own.iter().map(|lease| lease.role.clone()).collect();
    let other: Vec<String> = other.into_iter().map(held_elsewhere).collect();
    row(
        CapabilityState::Active,
        format!(
            "Machine {machine} holds {}. Another machine holds {}.",
            listed(&own),
            listed(&other)
        ),
        String::new(),
    )
}

pub(crate) async fn llm_worker_lease_capability(state: &AppState) -> CapabilityRow {
    let Some(persist) = state.persist() else {
        return row(
            CapabilityState::Off,
            "Persistence is disabled, so LLM worker roles are not leased".to_owned(),
            String::new(),
        );
    };
    let machine = persist.llm_machine_uuid().await;
    match persist.list_llm_worker_leases().await {
        Ok(leases) => llm_worker_lease_row(machine, &leases),
        Err(e) => row(
            CapabilityState::Degraded,
            format!("Could not read the LLM worker leases: {e}"),
            "Check that Postgres is reachable at the configured MIKE_DATABASE_URL".to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn lease(role: &str, machine: Uuid, age_secs: f64) -> WorkerLeaseRow {
        WorkerLeaseRow {
            role: role.to_owned(),
            machine_uuid: machine,
            heartbeat_at: Utc::now(),
            age_secs,
        }
    }

    #[test]
    fn row_without_machine_identity_is_degraded_with_a_fix() {
        let r = llm_worker_lease_row(None, &[]);
        assert_eq!(r.state, CapabilityState::Degraded);
        assert!(r.detail.contains("without a lease"), "{}", r.detail);
        assert!(r.fix.contains("machine.json"), "{}", r.fix);
    }

    #[test]
    fn row_with_no_claimed_roles_is_active() {
        let me = Uuid::new_v4();
        let r = llm_worker_lease_row(Some(me), &[]);
        assert_eq!(r.state, CapabilityState::Active);
        assert!(r.detail.contains("no LLM worker role"), "{}", r.detail);
    }

    #[test]
    fn row_names_own_roles_and_the_other_machines_roles() {
        let me = Uuid::new_v4();
        let other = Uuid::new_v4();
        let leases = [
            lease("judge_worker", me, 4.0),
            lease("session_judge", me, 9.0),
            lease("steerer", other, 12.4),
        ];
        let r = llm_worker_lease_row(Some(me), &leases);
        assert_eq!(r.state, CapabilityState::Active);
        assert!(
            r.detail.contains("holds judge_worker, session_judge."),
            "{}",
            r.detail
        );
        assert!(
            r.detail.contains(&format!(
                "steerer (machine {other}, last heartbeat 12 s ago)"
            )),
            "{}",
            r.detail
        );
    }

    #[test]
    fn row_with_every_role_held_elsewhere_says_this_machine_holds_no_role() {
        let me = Uuid::new_v4();
        let leases = [lease("steerer", Uuid::new_v4(), 1.0)];
        let r = llm_worker_lease_row(Some(me), &leases);
        assert!(r.detail.contains("holds no role."), "{}", r.detail);
    }
}
