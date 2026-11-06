mod apply;
mod atomic;
mod cli;
mod error;
mod locations;
mod mcp;
mod mods;
mod probe;
mod route;
mod settings;
mod shell;
mod shell_name;
mod status;

#[cfg(test)]
mod tests;

pub use cli::{
    ClaudeCodeArgs, ClaudeCodeCommand, McpArgs, ModsArgs, RouteArgs, ShellArgs, TrustArgs, run,
};
pub use route::Route;
pub use shell::Shell;

pub(crate) use apply::{
    ShellRequest, add_function, add_mods, choose_route, choose_trust, connect_mcp, disconnect_mcp,
    remove_functions, remove_mods,
};
pub(crate) use error::ClaudeCodeError;
pub(crate) use locations::Locations;
pub(crate) use mcp::{
    Registration, inspect as inspect_registration, inspect_url_for,
    locate_for_process as locate_mcp_binary,
};
pub(crate) use mods::{find_for_process as find_mods_source, list as list_mods};
pub(crate) use route::stored as stored_route;
pub(crate) use settings::{
    AUTO_TRUST_KEY, FUNCTION_KEY, ROUTE_KEY, SETUP_KEY, read as read_setting,
    read_bool as read_bool_setting, remove as remove_settings, write_text as write_setting,
};
pub(crate) use shell::{BEGIN as BLOCK_BEGIN, Function, mike_command, render_block};
pub(crate) use shell_name::suggest as suggest_function_name;
pub(crate) use status::gather as gather_status;

pub(crate) fn refuse_in_sandbox(action: &'static str) -> Result<(), ClaudeCodeError> {
    if crate::sandbox::active() {
        return Err(ClaudeCodeError::Sandbox { action });
    }
    Ok(())
}
