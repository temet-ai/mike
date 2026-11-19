use super::{Readings, health_row, problems};
use crate::web_base::capability::CapabilityState;

fn healthy() -> Readings {
    Readings {
        requests: 700,
        linked: 640,
        continues: 645,
        unresolved: 2,
        unknown_kind: 0,
        resends: 12,
        unseen_continuations: 0,
        label_shaped: 0,
        day_requests: 8_600,
        day_linked: 6_500,
    }
}

#[test]
fn health_row_today_readings_are_active() {
    let row = health_row(&healthy());
    assert!(matches!(row.state, CapabilityState::Active));
    assert!(row.fix.is_empty());
}

#[test]
fn health_row_unknown_thread_type_is_degraded_with_remedy() {
    let row = health_row(&Readings {
        unknown_kind: 1,
        ..healthy()
    });
    assert!(matches!(row.state, CapabilityState::Degraded));
    assert!(row.detail.contains("thread type mike does not know"));
    assert!(row.fix.contains("thread_link.rs"));
}

#[test]
fn problems_unresolved_links_over_share_are_named() {
    let found = problems(&Readings {
        unresolved: 40,
        ..healthy()
    });
    assert_eq!(found.len(), 1);
    assert!(found[0].contains("40 of 645"));
}

#[test]
fn problems_link_share_collapse_is_named() {
    let found = problems(&Readings {
        linked: 50,
        ..healthy()
    });
    assert!(found.iter().any(|p| p.contains("fell to")));
}

#[test]
fn problems_quiet_hour_does_not_judge_link_share() {
    let found = problems(&Readings {
        requests: 10,
        linked: 0,
        continues: 0,
        unresolved: 0,
        ..healthy()
    });
    assert!(found.is_empty());
}

#[test]
fn problems_label_shaped_subagent_calls_are_named() {
    let found = problems(&Readings {
        label_shaped: 39,
        ..healthy()
    });
    assert!(found[0].contains("39 agent label call"));
}
