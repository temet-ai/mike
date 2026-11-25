pub struct FleetPoint {
    pub point: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

pub const FLEET_POINTS: &[FleetPoint] = &[
    FleetPoint {
        point: "fleet_stop_check",
        label: "Fleet stop check",
        description: "Facts first, then Jev, decide whether a seat's stop goes through or is pushed back once.",
    },
    FleetPoint {
        point: "fleet_wake_check",
        label: "Fleet wake check",
        description: "Jev decides whether a mail or wakeup wakes a seat now or waits for its next turn.",
    },
    FleetPoint {
        point: "fleet_triage",
        label: "Fleet check triage",
        description: "Rules, then Jev, put a retry, fix-code or fix-environment line in a failed check's receipt.",
    },
    FleetPoint {
        point: "fleet_commit_message",
        label: "Fleet commit message",
        description: "Jev warns when a fleet_commit message does not match its diff; it never refuses.",
    },
    FleetPoint {
        point: "fleet_effort",
        label: "Fleet effort split",
        description: "Jev labels request content as work, machinery or waiting where the trigger does not decide it.",
    },
];
