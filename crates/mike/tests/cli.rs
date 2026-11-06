#[path = "cli/claude_wrapper.rs"]
mod claude_wrapper;
#[path = "cli/doctor.rs"]
mod doctor;
#[path = "cli/dotenv_db_url_real_boot.rs"]
mod dotenv_db_url_real_boot;
#[path = "cli/eval_import_holdout.rs"]
mod eval_import_holdout;
#[path = "cli/eval_withdraw_batch.rs"]
mod eval_withdraw_batch;

#[path = "cli/fleet_lifecycle.rs"]
mod fleet_lifecycle;
#[path = "cli/user_guide.rs"]
mod user_guide;
#[path = "cli/version.rs"]
mod version;

mod config {
    pub use mike::config::write;
}
#[path = "../src/tui/input.rs"]
mod setup_input;
#[path = "../src/setup/persistence.rs"]
mod setup_persistence;

#[path = "cli/db_fixture_contract.rs"]
mod db_fixture_contract;
#[path = "cli/installed_config.rs"]
mod installed_config;
#[path = "cli/local_bind.rs"]
mod local_bind;

#[path = "cli/setup.rs"]
mod setup;

#[path = "cli/lifecycle_contracts.rs"]
mod lifecycle_contracts;
