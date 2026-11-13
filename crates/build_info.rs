pub fn requested(args: impl Iterator<Item = std::ffi::OsString>) -> bool {
    args.eq(["--build-info"])
}

pub struct Identity<'a> {
    pub sha: &'a str,
    pub dirty: bool,
    pub built_at: &'a str,
}

pub fn print(identity: Identity<'_>) {
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1,
            "product": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "source_sha": identity.sha,
            "source_dirty": identity.dirty,
            "built_at": identity.built_at,
        })
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_info_fastpath_accepts_only_the_exact_standalone_flag() {
        for (args, expected) in [
            (vec!["--build-info"], true),
            (vec![], false),
            (vec!["--build-info", "serve"], false),
            (vec!["--config", "private.toml", "--build-info"], false),
            (vec!["serve", "--build-info"], false),
            (vec!["--build-info", "--build-info"], false),
        ] {
            assert_eq!(super::requested(args.into_iter().map(Into::into)), expected);
        }
    }
}
