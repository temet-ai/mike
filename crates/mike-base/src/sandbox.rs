pub const ENV_VAR: &str = "MIKE_SANDBOX";

pub fn active() -> bool {
    active_from(std::env::var(ENV_VAR).ok().as_deref())
}

pub fn active_from(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim).map(str::to_ascii_lowercase).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[cfg(test)]
pub mod tests {
    use super::active_from;

    #[test]
    fn only_explicit_truthy_values_switch_the_sandbox_on() {
        for on in ["1", "true", " TRUE ", "yes", "on"] {
            assert!(active_from(Some(on)), "{on}");
        }
        for off in ["", "0", "false", "no", "maybe"] {
            assert!(!active_from(Some(off)), "{off}");
        }
        assert!(!active_from(None));
    }
}
