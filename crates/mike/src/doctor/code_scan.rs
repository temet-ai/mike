use serde_json::Value;

use super::Check;

pub(super) const ROUTE: &str = "/api/code-findings?hours=24&limit=1";
const NOT_REPORTED: &str = "the inspect API did not report the code scan; start the proxy (`mike serve`) to see its switches, rules and last scan";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuiltinRules {
    pub(crate) count: usize,
    pub(crate) version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeScanFacts {
    pub(crate) scanning: bool,
    pub(crate) delivering: bool,
    pub(crate) pushing: bool,
    pub(crate) grammars: Vec<String>,
    pub(crate) rules: Option<BuiltinRules>,
    pub(crate) scans_24h: i64,
    pub(crate) last_scan_at: Option<String>,
}

impl CodeScanFacts {
    pub(crate) fn from_json(body: &Value) -> Option<Self> {
        let flag = |key: &str| body.get(key).and_then(Value::as_bool);
        let text_list = |key: &str| -> Vec<String> {
            body.get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let rules = body
            .get("rules_version")
            .and_then(Value::as_str)
            .map(|version| BuiltinRules {
                count: text_list("rule_ids").len(),
                version: version.to_string(),
            });
        Some(Self {
            scanning: flag("scanning")?,
            delivering: flag("delivering")?,
            pushing: flag("pushing")?,
            grammars: text_list("grammars"),
            rules,
            scans_24h: body.pointer("/summary/scans").and_then(Value::as_i64)?,
            last_scan_at: body
                .pointer("/summary/last_scan_at")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    pub(crate) fn rules_loaded(&self) -> bool {
        self.rules.is_some()
    }

    pub(crate) fn detail(&self) -> String {
        let on = |flag: bool| if flag { "on" } else { "off" };
        let rules = match &self.rules {
            Some(rules) => format!("{} built-in rules, version {}", rules.count, rules.version),
            None => "the built-in rules did not load, so no edit is scanned".to_string(),
        };
        let last = self.last_scan_at.as_deref().unwrap_or("never");
        format!(
            "change record and scan {}; telling sessions {}; push to idle sessions {}; {} grammars ({}); {rules}; last scan {last}; {} scans in 24 h",
            on(self.scanning),
            on(self.delivering),
            on(self.pushing),
            self.grammars.len(),
            self.grammars.join(", "),
            self.scans_24h,
        )
    }
}

pub(super) fn check(facts: Option<&CodeScanFacts>) -> Check {
    match facts {
        None => Check::ok("code_scan", "code scan", NOT_REPORTED),
        Some(facts) if facts.rules_loaded() => Check::ok("code_scan", "code scan", facts.detail()),
        Some(facts) => Check::fail("code_scan", "code scan", facts.detail(), None),
    }
}

pub(super) fn probe_check(facts: Option<&CodeScanFacts>) -> super::installation::Check {
    use super::installation::Check as Probe;
    match facts {
        None => Probe::new(
            "code_scan",
            false,
            "unavailable",
            "CODE_SCAN_NOT_REPORTED",
            NOT_REPORTED,
        ),
        Some(facts) if facts.rules_loaded() => Probe::new(
            "code_scan",
            false,
            "pass",
            "CODE_SCAN_REPORTED",
            facts.detail(),
        ),
        Some(facts) => Probe::new(
            "code_scan",
            true,
            "fail",
            "CODE_SCAN_RULES_NOT_LOADED",
            facts.detail(),
        ),
    }
}
