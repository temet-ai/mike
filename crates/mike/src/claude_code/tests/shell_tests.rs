use std::path::Path;

use super::super::atomic::Removal;
use super::super::error::ClaudeCodeError;
use super::super::locations::Locations;
use super::super::route::Route;
use super::super::shell::{
    BEGIN, END, Function, Shell, has_block, install, render_block, uninstall, with_block,
    without_block,
};
use super::super::shell_name::{suggest, validate};
use super::Machine;

fn function<'a>(name: &'a str, route: Route) -> Function<'a> {
    Function {
        name,
        route,
        proxy_addr: "127.0.0.1:9988",
        mike: "mike",
    }
}

#[test]
fn a_posix_shell_gets_a_function_that_calls_mike_claude() {
    for shell in [Shell::Zsh, Shell::Bash] {
        let block = render_block(shell, &function("cc", Route::Sidecar));
        assert!(block.starts_with(&format!("{BEGIN}\n")), "{block}");
        assert!(block.ends_with(&format!("{END}\n")), "{block}");
        assert!(block.contains("cc() {"), "{block}");
        assert!(block.contains("command mike claude \"$@\""), "{block}");
    }
}

#[test]
fn the_plain_route_sets_the_base_url_without_a_v1_suffix() {
    let block = render_block(Shell::Zsh, &function("cc", Route::Plain));
    assert!(
        block.contains("ANTHROPIC_BASE_URL=http://127.0.0.1:9988 command claude \"$@\""),
        "{block}"
    );
    assert!(!block.contains("/v1"), "{block}");
    assert!(!block.contains("mike claude"), "{block}");
}

#[test]
fn fish_gets_fish_syntax_for_both_routes() {
    let sidecar = render_block(Shell::Fish, &function("cc", Route::Sidecar));
    assert!(
        sidecar.contains("function cc\n    command mike claude $argv\nend"),
        "{sidecar}"
    );
    let plain = render_block(Shell::Fish, &function("cc", Route::Plain));
    assert!(
        plain.contains("env ANTHROPIC_BASE_URL=http://127.0.0.1:9988 claude $argv"),
        "{plain}"
    );
}

#[test]
fn a_block_is_appended_after_a_blank_line_and_replaced_in_place() -> anyhow::Result<()> {
    let path = Path::new("rc");
    let first = render_block(Shell::Zsh, &function("cc", Route::Sidecar));
    let appended = with_block(path, "export A=1\n", &first)?;
    assert_eq!(appended, format!("export A=1\n\n{first}"));
    let second = render_block(Shell::Zsh, &function("mcc", Route::Plain));
    let with_tail = format!("{appended}export B=2\n");
    let replaced = with_block(path, &with_tail, &second)?;
    assert_eq!(replaced, format!("export A=1\n\n{second}export B=2\n"));
    assert_eq!(with_block(path, &replaced, &second)?, replaced);
    Ok(())
}

#[test]
fn a_block_goes_into_an_empty_file_without_a_leading_blank_line() -> anyhow::Result<()> {
    let block = render_block(Shell::Bash, &function("cc", Route::Sidecar));
    assert_eq!(with_block(Path::new("rc"), "", &block)?, block);
    Ok(())
}

#[test]
fn removing_the_block_restores_the_original_text() -> anyhow::Result<()> {
    let path = Path::new("rc");
    let block = render_block(Shell::Zsh, &function("cc", Route::Sidecar));
    for original in ["export A=1\n", "export A=1", "", "a\n\nb\n"] {
        let added = with_block(path, original, &block)?;
        let removed = without_block(path, &added)?;
        let expected = if original.is_empty() || original.ends_with('\n') {
            original.to_string()
        } else {
            format!("{original}\n")
        };
        assert_eq!(removed, Some(expected), "original {original:?}");
    }
    assert_eq!(without_block(path, "export A=1\n")?, None);
    Ok(())
}

#[test]
fn a_begin_marker_without_an_end_marker_is_refused() {
    let text = format!("{BEGIN}\ncc() {{ :; }}\n");
    for result in [
        with_block(Path::new("rc"), &text, "x\n").map(|_| ()),
        without_block(Path::new("rc"), &text).map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(ClaudeCodeError::BrokenBlock { .. })),
            "{result:?}"
        );
    }
}

#[test]
fn install_and_uninstall_round_trip_through_a_symlinked_rc_file() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let real = machine.home().join("dotfiles").join("zshrc");
    std::fs::create_dir_all(real.parent().ok_or_else(|| anyhow::anyhow!("no parent"))?)?;
    std::fs::write(&real, "export A=1\n")?;
    let link = machine.home().join(".zshrc");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link)?;
    let block = render_block(Shell::Zsh, &function("cc", Route::Sidecar));
    install(&link, &block)?;
    assert!(has_block(&link)?);
    assert!(std::fs::read_to_string(&real)?.contains(BEGIN));
    assert_eq!(uninstall(&link)?, Removal::Removed);
    assert_eq!(std::fs::read_to_string(&real)?, "export A=1\n");
    assert_eq!(uninstall(&link)?, Removal::NotThere);
    Ok(())
}

#[test]
fn the_rc_file_depends_on_the_shell_and_the_system() {
    let mut loc = Locations::for_home(Path::new("/h"), None);
    assert_eq!(Shell::Zsh.rc_path(&loc, "linux"), Path::new("/h/.zshrc"));
    assert_eq!(Shell::Bash.rc_path(&loc, "linux"), Path::new("/h/.bashrc"));
    assert_eq!(
        Shell::Bash.rc_path(&loc, "macos"),
        Path::new("/h/.bash_profile")
    );
    assert_eq!(
        Shell::Fish.rc_path(&loc, "linux"),
        Path::new("/h/.config/fish/config.fish")
    );
    loc.zdotdir = Some(Path::new("/z").to_path_buf());
    loc.xdg_config_home = Some(Path::new("/x").to_path_buf());
    assert_eq!(Shell::Zsh.rc_path(&loc, "linux"), Path::new("/z/.zshrc"));
    assert_eq!(
        Shell::Fish.rc_path(&loc, "linux"),
        Path::new("/x/fish/config.fish")
    );
}

#[test]
fn the_login_shell_is_read_from_the_shell_variable() {
    assert_eq!(Shell::from_login_shell(Some("/bin/zsh")), Some(Shell::Zsh));
    assert_eq!(
        Shell::from_login_shell(Some("/usr/local/bin/fish")),
        Some(Shell::Fish)
    );
    assert_eq!(Shell::from_login_shell(Some("/bin/dash")), None);
    assert_eq!(Shell::from_login_shell(None), None);
}

#[test]
fn a_name_that_is_a_command_on_path_is_refused() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    machine.add_executable("cc", "#!/bin/sh\n")?;
    let path = machine.locations.path_var.as_deref();
    let error = validate("cc", path)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected a refusal"))?;
    assert!(
        error.to_string().contains("already a command on PATH"),
        "{error}"
    );
    assert!(validate("mcc", path).is_ok());
    Ok(())
}

#[test]
fn names_that_are_not_identifiers_or_are_shell_words_are_refused() {
    for bad in ["", "1cc", "c c", "c;c", "if", "cd", "a/b", &"x".repeat(40)] {
        assert!(validate(bad, None).is_err(), "{bad:?}");
    }
    assert!(validate("my_cc-2", None).is_ok());
}

#[test]
fn the_suggested_name_is_cc_unless_it_is_taken() -> anyhow::Result<()> {
    let machine = Machine::new()?;
    let path = machine.locations.path_var.as_deref();
    assert_eq!(suggest(path), "cc");
    machine.add_executable("cc", "#!/bin/sh\n")?;
    assert_eq!(suggest(path), "mcc");
    machine.add_executable("mcc", "#!/bin/sh\n")?;
    assert_eq!(suggest(path), "cmike");
    Ok(())
}
