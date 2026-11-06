pub fn requested(args: impl Iterator<Item = std::ffi::OsString>) -> bool {
    args.eq(["--build-info"])
}

pub fn print() {
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1,
            "product": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "source_sha": env!("MIKE_BUILD_SHA"),
            "source_dirty": env!("MIKE_BUILD_DIRTY") != "false",
            "built_at": env!("MIKE_BUILD_TIME"),
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
