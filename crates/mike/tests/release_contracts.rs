pub use mike::config;

#[path = "../src/setup/docker_command.rs"]
mod docker_command;

#[path = "../src/setup/mcp/mod.rs"]
pub mod mcp;

#[path = "../src/stack/protocol.rs"]
mod protocol;

#[path = "../src/stack/lease.rs"]
mod lease;

#[path = "../src/stack/socket_path.rs"]
mod socket_path;

#[cfg(unix)]
pub use mike::config::{paths, write};
#[cfg(unix)]
#[path = "../src/owner_only_file.rs"]
mod owner_only_file;
#[cfg(unix)]
#[path = "../src/config/write_transaction.rs"]
mod write_transaction;

#[path = "../src/stack/startup_policy.rs"]
mod startup_policy;

#[path = "../src/config/endpoint.rs"]
mod endpoint;
pub use mike::config::ProxyConfig;

#[path = "../src/config/paths.rs"]
mod installation_paths;

#[path = "../src/api/handlers/spa_delta_fallback.rs"]
mod spa_delta_fallback;

#[path = "release/spa_delta_fallback_routes.rs"]
mod spa_delta_fallback_routes;
