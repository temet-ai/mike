use super::manager::Manager;
use super::status::{self, UnitFacts};
use crate::db_provision::SystemRunner;

#[derive(Debug, Clone)]
pub(crate) struct BackgroundFacts {
    pub(crate) manager_name: &'static str,
    pub(crate) units: Vec<UnitFacts>,
}

pub(crate) async fn observe() -> Option<BackgroundFacts> {
    let manager = Manager::from_process().ok()?;
    Some(BackgroundFacts {
        manager_name: manager.platform.manager_name(),
        units: status::collect(&SystemRunner, &manager).await,
    })
}
