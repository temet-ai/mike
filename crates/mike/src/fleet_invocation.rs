use std::ffi::OsString;

const RETIRED_SHIM_VERB: &str = "build-slot";
const RETIRED_SHIM_PROGRAM: &str = "cargo";
const RETIRED_BACKGROUND_HOOK: &str = "cargo-background-hook";

pub(crate) fn timed_run(
    mut args: impl Iterator<Item = OsString>,
) -> Option<(OsString, Vec<OsString>)> {
    if args.next()? != "fleet" {
        return None;
    }
    let verb = args.next()?;
    if verb == "timed-run" {
        Some((args.next()?, args.collect()))
    } else if verb == RETIRED_SHIM_VERB {
        Some((RETIRED_SHIM_PROGRAM.into(), args.collect()))
    } else {
        None
    }
}

pub(crate) fn background_hook(args: impl Iterator<Item = OsString>) -> bool {
    let args: Vec<_> = args.collect();
    args == ["fleet", "background-hook"] || args == ["fleet", RETIRED_BACKGROUND_HOOK]
}

pub(crate) fn receipt_hook(args: impl Iterator<Item = OsString>) -> bool {
    args.eq(["fleet", "receipt-hook"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[&str]) -> Vec<OsString> {
        args.iter().map(Into::into).collect()
    }

    #[test]
    fn background_hook_fastpath_accepts_only_the_exact_local_command() {
        for args in [
            vec!["fleet", "background-hook", "extra"],
            vec!["fleet", "cargo-background-hook", "extra"],
            vec!["fleet", "status"],
            vec!["--help", "fleet", "background-hook"],
            vec![],
        ] {
            assert!(!background_hook(words(&args).into_iter()));
        }
        for verb in ["background-hook", "cargo-background-hook"] {
            assert!(background_hook(words(&["fleet", verb]).into_iter()));
        }
    }

    #[test]
    fn a_timed_run_names_its_program_and_a_shim_from_before_names_the_old_one() {
        let run = |args: &[&str]| timed_run(words(args).into_iter());
        assert_eq!(
            run(&["fleet", "timed-run", "make", "all"]),
            Some(("make".into(), words(&["all"])))
        );
        assert_eq!(
            run(&["fleet", "build-slot", "check"]),
            Some(("cargo".into(), words(&["check"])))
        );
        assert_eq!(run(&["fleet", "timed-run"]), None);
        assert_eq!(run(&["fleet", "status"]), None);
        assert_eq!(run(&["spawn", "timed-run", "make"]), None);
    }
}
