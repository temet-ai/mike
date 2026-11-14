pub use mike_fleet::launch;
pub use mike_fleet::*;
pub use mike_lessons::*;
pub use mike_lessons::{
    analysis, dossier, extract, memory_cap, observability, settings, workspace,
};
pub mod api;
pub mod app;
pub mod claude;
pub mod claude_code;
pub mod cli;
pub mod config;
pub mod db_listener;
pub mod db_provision;
pub mod doctor;
pub mod feature_presets;
pub mod forward;
pub mod retention_worker;
pub(crate) mod scheduled_jobs;
pub mod serve;
pub mod service;
pub mod setup;
pub(crate) mod spa;
pub mod stack;
pub mod store;
pub mod tui;
pub mod uninstall;
pub mod user_config;
pub mod wright;

#[cfg(test)]
mod cross_crate_tests;
