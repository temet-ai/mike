use std::sync::Arc;

use crate::filter::service::FilterService;
use crate::settings::service::SettingsService;
use crate::state::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Clone)]
pub(crate) struct Owners {
    pub(crate) persist: PersistHandle,
    pub(crate) settings: Option<Arc<SettingsService>>,
    pub(crate) filters: Option<Arc<FilterService>>,
}

impl Owners {
    pub(crate) fn from_state(state: &AppState) -> Option<Self> {
        Some(Self {
            persist: state.persist()?.clone(),
            settings: state.settings_service().cloned(),
            filters: state.filter_service().cloned(),
        })
    }

    pub(crate) fn settings(&self) -> Result<&SettingsService, Halt> {
        self.settings.as_deref().ok_or_else(|| {
            Halt::Transient("the settings service is not running in this process".to_owned())
        })
    }

    pub(crate) fn filters(&self) -> Result<&FilterService, Halt> {
        self.filters.as_deref().ok_or_else(|| {
            Halt::Transient("the filter rule service is not running in this process".to_owned())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Halt {
    Refused(String),
    Failed(String),
    Transient(String),
}

impl From<PersistError> for Halt {
    fn from(e: PersistError) -> Self {
        Halt::Transient(e.to_string())
    }
}
