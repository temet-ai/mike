mod binary;
mod commands;
mod endpoints;
mod environment;
mod error;
mod exec;
mod facts;
mod files;
mod guard;
mod health;
mod install;
mod launchd;
mod manager;
mod platform;
mod restart;
mod setup_api;
pub(crate) mod spec;
mod state;
pub(crate) mod status;
mod supervision;
mod systemd;
mod uninstall;

#[cfg(test)]
mod tests;

pub use commands::{DryRunArgs, ServiceArgs, ServiceCommand, run};

pub(crate) use binary::stable_path as stable_binary_path;
pub(crate) use environment::{Inputs as ServiceInputs, service_path};
pub(crate) use facts::{BackgroundFacts, observe};
pub(crate) use files::Presence;
pub(crate) use setup_api::{
    Collected, Observation, install_for_setup, observe_for_setup, restart_for_setup,
};
pub(crate) use state::Activity;
pub(crate) use supervision::{InstalledProxy, installed_proxy, start_proxy};
