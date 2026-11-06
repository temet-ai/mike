pub(super) mod claude;
pub(super) mod database;
pub(super) mod embeddings;
pub(super) mod features;
pub(super) mod machine;
pub(super) mod models;
mod proxy;
pub(super) mod ready;
pub(super) mod service;
pub(super) mod terminal;
pub(super) mod welcome;

use super::step::WizardStep;

pub(super) fn all() -> Vec<Box<dyn WizardStep>> {
    vec![
        Box::new(welcome::Welcome::new()),
        Box::new(machine::Machine::new()),
        Box::new(database::Database::new()),
        Box::new(service::Service::new()),
        Box::new(claude::Claude::new()),
        Box::new(features::Features::new()),
        Box::new(models::Models::new()),
        Box::new(embeddings::Embeddings::new()),
        Box::new(terminal::Terminal::real()),
        Box::new(ready::Ready::new()),
    ]
}
