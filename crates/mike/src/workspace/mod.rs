use std::str::FromStr;

pub mod boot_id;
pub(crate) mod git_repo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisteredVia {
    Hook,
    Mcp,
    Rest,
    Passive,
    Operator,
    Launcher,
}

impl RegisteredVia {
    pub(crate) fn as_db_str(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::Mcp => "mcp",
            Self::Rest => "rest",
            Self::Passive => "passive",
            Self::Operator => "operator",
            Self::Launcher => "launcher",
        }
    }
}

impl FromStr for RegisteredVia {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "hook" => Ok(Self::Hook),
            "mcp" => Ok(Self::Mcp),
            "rest" => Ok(Self::Rest),
            "passive" => Ok(Self::Passive),
            "operator" => Ok(Self::Operator),
            "launcher" => Ok(Self::Launcher),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Live,
    Stale,
    Ended,
    Uncaptured,
}

impl Liveness {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Stale => "stale",
            Self::Ended => "ended",
            Self::Uncaptured => "uncaptured",
        }
    }

    pub(crate) fn from_db(s: &str) -> Self {
        match s {
            "ended" => Self::Ended,
            "live" => Self::Live,
            "uncaptured" => Self::Uncaptured,
            _ => Self::Stale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosterScope {
    Live,
    Active,
    All,
}

impl RosterScope {
    pub(crate) fn filter_bools(self) -> (bool, bool) {
        match self {
            Self::Live => (false, false),
            Self::Active => (true, false),
            Self::All => (true, true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_via_round_trips_through_db_str() {
        for v in [
            RegisteredVia::Hook,
            RegisteredVia::Mcp,
            RegisteredVia::Rest,
            RegisteredVia::Passive,
            RegisteredVia::Operator,
            RegisteredVia::Launcher,
        ] {
            assert_eq!(v.as_db_str().parse::<RegisteredVia>(), Ok(v));
        }
    }

    #[test]
    fn registered_via_rejects_unknown_value() {
        assert!("carrier-pigeon".parse::<RegisteredVia>().is_err());
    }

    #[test]
    fn liveness_as_str_matches_wire_vocabulary() {
        assert_eq!(Liveness::Live.as_str(), "live");
        assert_eq!(Liveness::Stale.as_str(), "stale");
        assert_eq!(Liveness::Ended.as_str(), "ended");
        assert_eq!(Liveness::Uncaptured.as_str(), "uncaptured");
        assert_eq!(Liveness::from_db("uncaptured"), Liveness::Uncaptured);
        assert_eq!(Liveness::from_db("bogus"), Liveness::Stale);
    }
}
