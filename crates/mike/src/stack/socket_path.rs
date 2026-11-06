use std::path::Path;

pub(crate) fn validate(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        path.as_os_str().as_encoded_bytes().len() <= 103,
        "private control socket path exceeds the portable Unix limit; select a configuration in a shorter absolute directory"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_socket_limit_counts_bytes_and_keeps_the_full_selected_path() {
        assert!(validate(Path::new(&"a".repeat(103))).is_ok());
        assert!(validate(Path::new(&"a".repeat(104))).is_err());
        assert!(validate(Path::new(&"λ".repeat(51))).is_ok());
        assert!(validate(Path::new(&"λ".repeat(52))).is_err());
    }
}
