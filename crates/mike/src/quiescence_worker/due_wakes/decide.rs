#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    Reachable,
    NotIdle,
    NoStream,
}

impl Reach {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Reachable => "reachable",
            Self::NotIdle => "not_idle",
            Self::NoStream => "no_stream",
        }
    }
}

pub(crate) fn classify_reach(turn_closed: bool, moved_on: bool, stream_open: bool) -> Reach {
    if turn_closed || moved_on {
        Reach::NotIdle
    } else if !stream_open {
        Reach::NoStream
    } else {
        Reach::Reachable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WakeFacts<'a> {
    pub(crate) mode: Option<&'a str>,
    pub(crate) delivered: bool,
    pub(crate) reverted: bool,
    pub(crate) expired: bool,
    pub(crate) reach: Reach,
    pub(crate) unbacked_wait: bool,
    pub(crate) backed_now: bool,
    pub(crate) pushed_last_hour: i64,
    pub(crate) cap: i64,
    pub(crate) may_wake: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WakeAction {
    Resolve { state: &'static str, expire: bool },
    Push,
    BackToPending,
}

pub(crate) fn decide_due_wake(f: &WakeFacts<'_>) -> WakeAction {
    let resolve = |state, expire| WakeAction::Resolve { state, expire };
    if f.mode != Some("wake") {
        return resolve("disabled", true);
    }
    if f.delivered {
        return resolve("appended", false);
    }
    if f.reverted {
        return resolve("reverted", false);
    }
    if f.expired {
        return resolve("expired", false);
    }
    if f.reach == Reach::NotIdle {
        return resolve("unreachable", false);
    }
    if f.unbacked_wait && f.backed_now {
        return resolve("backed", true);
    }
    if f.pushed_last_hour >= f.cap {
        return resolve("capped_hour", false);
    }
    if f.reach == Reach::NoStream {
        return resolve("unreachable", false);
    }
    if !f.may_wake {
        return WakeAction::BackToPending;
    }
    WakeAction::Push
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> WakeFacts<'static> {
        WakeFacts {
            mode: Some("wake"),
            delivered: false,
            reverted: false,
            expired: false,
            reach: Reach::Reachable,
            unbacked_wait: true,
            backed_now: false,
            pushed_last_hour: 0,
            cap: 3,
            may_wake: true,
        }
    }

    #[test]
    fn decide_due_wake_first_match_wins() {
        assert_eq!(decide_due_wake(&facts()), WakeAction::Push);
        assert_eq!(
            decide_due_wake(&WakeFacts {
                mode: Some("append"),
                ..facts()
            }),
            WakeAction::Resolve {
                state: "disabled",
                expire: true
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                delivered: true,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "appended",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                reverted: true,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "reverted",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                expired: true,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "expired",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                reach: Reach::NotIdle,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "unreachable",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                backed_now: true,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "backed",
                expire: true
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                pushed_last_hour: 3,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "capped_hour",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                reach: Reach::NoStream,
                ..facts()
            }),
            WakeAction::Resolve {
                state: "unreachable",
                expire: false
            }
        );
        assert_eq!(
            decide_due_wake(&WakeFacts {
                may_wake: false,
                ..facts()
            }),
            WakeAction::BackToPending
        );
    }

    #[test]
    fn classify_reach_same_rule_for_both_arms() {
        assert_eq!(classify_reach(true, false, true), Reach::NotIdle);
        assert_eq!(classify_reach(false, true, true), Reach::NotIdle);
        assert_eq!(classify_reach(false, false, false), Reach::NoStream);
        assert_eq!(classify_reach(false, false, true), Reach::Reachable);
    }
}
