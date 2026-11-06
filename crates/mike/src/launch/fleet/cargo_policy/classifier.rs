use std::ffi::OsString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Query,
    Format,
    HeavyOrUnknown,
}

pub fn classify(arguments: &[OsString]) -> Decision {
    let Some((index, command)) = command(arguments) else {
        return Decision::HeavyOrUnknown;
    };
    if command == "fmt" {
        return Decision::Format;
    }
    if matches!(
        command,
        "--version"
            | "-V"
            | "--list"
            | "--help"
            | "-h"
            | "version"
            | "metadata"
            | "locate-project"
            | "pkgid"
            | "tree"
            | "read-manifest"
    ) {
        return Decision::Query;
    }
    if command == "help" {
        return match arguments.get(index + 1).and_then(|value| value.to_str()) {
            None if arguments.len() == index + 1 => Decision::Query,
            Some(builtin) if known_builtin(builtin) => Decision::Query,
            _ => Decision::HeavyOrUnknown,
        };
    }
    if known_builtin(command)
        && arguments
            .get(index + 1)
            .and_then(|value| value.to_str())
            .is_some_and(|value| matches!(value, "--help" | "-h"))
    {
        return Decision::Query;
    }
    Decision::HeavyOrUnknown
}

fn known_builtin(command: &str) -> bool {
    matches!(
        command,
        "add"
            | "bench"
            | "build"
            | "check"
            | "clean"
            | "doc"
            | "fetch"
            | "fix"
            | "generate-lockfile"
            | "help"
            | "info"
            | "init"
            | "install"
            | "locate-project"
            | "login"
            | "logout"
            | "metadata"
            | "new"
            | "owner"
            | "package"
            | "pkgid"
            | "publish"
            | "read-manifest"
            | "remove"
            | "report"
            | "run"
            | "rustc"
            | "rustdoc"
            | "search"
            | "test"
            | "tree"
            | "uninstall"
            | "update"
            | "vendor"
            | "verify-project"
            | "version"
            | "yank"
    )
}

fn command(arguments: &[OsString]) -> Option<(usize, &str)> {
    if arguments.is_empty() {
        return Some((0, "--help"));
    }
    let mut at = usize::from(arguments.first()?.to_str()?.starts_with('+'));
    while let Some(argument) = arguments.get(at).and_then(|value| value.to_str()) {
        if argument == "--" {
            return None;
        }
        if matches!(argument, "--version" | "-V" | "--list" | "--help" | "-h")
            || !argument.starts_with('-')
        {
            return Some((at, argument));
        }
        at += prefix_width(argument)?;
    }
    None
}

fn prefix_width(argument: &str) -> Option<usize> {
    if matches!(
        argument,
        "--verbose" | "--quiet" | "--frozen" | "--locked" | "--offline"
    ) {
        return Some(1);
    }
    let flags = argument.strip_prefix('-')?;
    if !flags.is_empty() && flags.bytes().all(|byte| matches!(byte, b'v' | b'q')) {
        return Some(1);
    }
    if matches!(
        argument,
        "--color" | "--config" | "--manifest-path" | "-C" | "-Z"
    ) {
        return Some(2);
    }
    if ["--color=", "--config=", "--manifest-path="]
        .iter()
        .any(|prefix| argument.starts_with(prefix))
        || (argument.starts_with("-Z") || argument.starts_with("-C")) && argument.len() > 2
    {
        return Some(1);
    }
    None
}

#[cfg(feature = "test-seams")]
#[path = "classifier_cases.rs"]
pub mod cases;
