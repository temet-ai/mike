pub(crate) fn ok_or_warn<T, E: std::fmt::Display>(
    result: Result<T, E>,
    step: &'static str,
) -> Option<T> {
    result
        .map_err(|e| tracing::warn!(step, error = %e, "background step failed"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::ok_or_warn;

    #[test]
    fn ok_or_warn_keeps_the_value_and_drops_the_error() {
        assert_eq!(ok_or_warn(Ok::<i32, String>(3), "step"), Some(3));
        assert_eq!(
            ok_or_warn(Err::<i32, String>("boom".to_owned()), "step"),
            None
        );
    }
}
