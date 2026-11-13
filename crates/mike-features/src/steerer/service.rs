use crate::store::persistence::steerer_repo::SteererPersist as _;
use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::Mutex;

use super::registry::{Steerer, SteererRegistry};
use crate::change_log::{ChangeSource, current_build};
use crate::config::SteeringWriteGuard;
use crate::eval::bundles::{BundleCause, BundlePublisher};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug)]
pub struct SteererEnvelopeOutput {
    pub steerer: Steerer,
    pub warnings: Vec<String>,
}

pub struct SteererFields<'a> {
    pub display_name: &'a str,
    pub description: &'a str,
    pub enabled: bool,
    pub authority_code: &'a str,
    pub mode_code: &'a str,
    pub model: &'a str,
    pub priority: i32,
    pub config: &'a serde_json::Value,
    pub prompt: Option<&'a str>,
    pub trigger_point: &'a str,
    pub invocation: &'a str,
    pub pattern: Option<&'a str>,
    pub project_id: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum SteeringServiceError {
    #[error("no steerer with that id")]
    NotFound,

    #[error("persistence error: {0}")]
    Persistence(#[from] PersistError),
}

pub struct SteeringService {
    registry: Arc<ArcSwap<SteererRegistry>>,

    write_lock: Arc<Mutex<SteeringWriteGuard>>,

    persist: PersistHandle,

    publisher: Option<Arc<BundlePublisher>>,
}

impl SteeringService {
    pub async fn reconcile_on_startup(
        registry: Arc<ArcSwap<SteererRegistry>>,
        write_lock: Arc<Mutex<SteeringWriteGuard>>,
        persist: PersistHandle,
        publisher: Option<Arc<BundlePublisher>>,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Arc::new(Self {
            registry,
            write_lock,
            persist,
            publisher,
        });

        {
            let _guard = service.write_lock.lock().await;
            let rows = service.persist.steerer().list_steerers().await?;
            service.rebuild_arcswap(rows);
        }

        Ok(service)
    }

    pub async fn create(
        &self,
        name: &str,
        agent_id: Option<i64>,
        fields: SteererFields<'_>,
        source: &ChangeSource,
    ) -> Result<SteererEnvelopeOutput, SteeringServiceError> {
        let source_json = source.to_setting_json(&current_build());
        let _guard = self.write_lock.lock().await;

        let id = match agent_id {
            Some(aid) => {
                self.insert_steerer_for_existing_agent(name, aid, &fields, &source_json)
                    .await?
            }
            None => {
                self.insert_steerer_with_new_agent(name, &fields, &source_json)
                    .await?
            }
        };

        let rows = self.persist.steerer().list_steerers().await?;
        self.rebuild_arcswap(rows);
        self.republish(id).await;

        let row = self
            .persist
            .steerer()
            .fetch_steerer_by_id(id)
            .await?
            .ok_or(SteeringServiceError::NotFound)?;
        let steerer = Self::convert_row_to_steerer(id, &row)?;

        tracing::info!(steerer_id = id, name = %steerer.name, "steerer created");

        Ok(SteererEnvelopeOutput {
            steerer,
            warnings: Vec::new(),
        })
    }

    fn convert_row_to_steerer(
        id: i64,
        row: &crate::store::persistence::steering::SteererRow,
    ) -> Result<Steerer, SteeringServiceError> {
        Steerer::try_from(row).map_err(|e| {
            tracing::error!(error = %e, steerer_id = id, "steerer domain conversion failed after insert");
            SteeringServiceError::Persistence(PersistError::DataInvariant(e))
        })
    }

    async fn insert_steerer_for_existing_agent(
        &self,
        name: &str,
        agent_id: i64,
        fields: &SteererFields<'_>,
        source_json: &str,
    ) -> Result<i64, SteeringServiceError> {
        let (id, _uuid) = self
            .persist
            .steerer()
            .insert_steerer(
                agent_id,
                crate::store::persistence::steering::SteererFields {
                    name,
                    display_name: fields.display_name,
                    description: fields.description,
                    authority_code: fields.authority_code,
                    mode_code: fields.mode_code,
                    model: fields.model,
                    priority: fields.priority,
                    enabled: fields.enabled,
                    config: fields.config,
                    prompt: fields.prompt,
                    trigger_point: fields.trigger_point,
                    invocation: fields.invocation,
                    pattern: fields.pattern,
                    project_id: fields.project_id,
                },
                Some(source_json),
            )
            .await
            .map_err(Self::map_existing_agent_insert_error)?;
        Ok(id)
    }

    fn map_existing_agent_insert_error(e: PersistError) -> SteeringServiceError {
        tracing::error!(error = %e, "steerer insert failed");
        SteeringServiceError::Persistence(e)
    }

    async fn insert_steerer_with_new_agent(
        &self,
        name: &str,
        fields: &SteererFields<'_>,
        source_json: &str,
    ) -> Result<i64, SteeringServiceError> {
        let (id, _uuid, bootstrapped_agent_id) = self
            .persist
            .steerer()
            .insert_steerer_with_bootstrapped_agent(
                crate::store::persistence::steering::SteererFields {
                    name,
                    display_name: fields.display_name,
                    description: fields.description,
                    authority_code: fields.authority_code,
                    mode_code: fields.mode_code,
                    model: fields.model,
                    priority: fields.priority,
                    enabled: fields.enabled,
                    config: fields.config,
                    prompt: fields.prompt,
                    trigger_point: fields.trigger_point,
                    invocation: fields.invocation,
                    pattern: fields.pattern,
                    project_id: fields.project_id,
                },
                Some(source_json),
            )
            .await
            .map_err(Self::map_bootstrap_insert_error)?;
        Self::log_bootstrapped_agent(id, bootstrapped_agent_id);
        Ok(id)
    }

    fn map_bootstrap_insert_error(e: PersistError) -> SteeringServiceError {
        tracing::error!(error = %e, "steerer insert with bootstrapped agent failed");
        SteeringServiceError::Persistence(e)
    }

    fn log_bootstrapped_agent(steerer_id: i64, agent_id: i64) {
        tracing::info!(steerer_id, agent_id, "bootstrapped agent for new steerer");
    }

    pub async fn update(
        &self,
        id: i64,
        fields: SteererFields<'_>,
        source: &ChangeSource,
    ) -> Result<SteererEnvelopeOutput, SteeringServiceError> {
        let source_json = source.to_setting_json(&current_build());
        let _guard = self.write_lock.lock().await;

        match self
            .persist
            .steerer()
            .update_steerer(
                crate::store::persistence::steering::SteererUpdate {
                    id,
                    display_name: fields.display_name,
                    description: fields.description,
                    enabled: fields.enabled,
                    authority_code: fields.authority_code,
                    mode_code: fields.mode_code,
                    model: fields.model,
                    priority: fields.priority,
                    config: fields.config,
                    prompt: fields.prompt,
                    trigger_point: fields.trigger_point,
                    invocation: fields.invocation,
                    pattern: fields.pattern,
                    project_id: fields.project_id,
                },
                Some(&source_json),
            )
            .await
        {
            Ok(true) => {}
            Ok(false) => return Err(SteeringServiceError::NotFound),
            Err(e) => {
                tracing::error!(error = %e, steerer_id = id, "steerer update failed");
                return Err(SteeringServiceError::Persistence(e));
            }
        }

        let rows = self.persist.steerer().list_steerers().await?;
        self.rebuild_arcswap(rows);
        self.republish(id).await;

        let row = self
            .persist
            .steerer()
            .fetch_steerer_by_id(id)
            .await?
            .ok_or(SteeringServiceError::NotFound)?;

        let steerer = Steerer::try_from(&row).map_err(|e| {
            tracing::error!(error = %e, steerer_id = id, "steerer domain conversion failed after update");
            SteeringServiceError::Persistence(PersistError::DataInvariant(e))
        })?;

        Ok(SteererEnvelopeOutput {
            steerer,
            warnings: Vec::new(),
        })
    }

    pub async fn delete(
        &self,
        id: i64,
        source: &ChangeSource,
    ) -> Result<(), SteeringServiceError> {
        let source_json = source.to_setting_json(&current_build());
        let _guard = self.write_lock.lock().await;

        match self
            .persist
            .steerer()
            .delete_steerer(id, Some(&source_json))
            .await
        {
            Ok(true) => {}
            Ok(false) => return Err(SteeringServiceError::NotFound),
            Err(e) => {
                tracing::error!(error = %e, steerer_id = id, "steerer delete failed");
                return Err(SteeringServiceError::Persistence(e));
            }
        }

        let rows = self.persist.steerer().list_steerers().await?;
        self.rebuild_arcswap(rows);
        self.republish(id).await;

        Ok(())
    }

    pub async fn set_enabled_preserving_config(
        &self,
        id: i64,
        enabled: bool,
        source: &ChangeSource,
    ) -> Result<(), SteeringServiceError> {
        let source_json = source.to_setting_json(&current_build());
        let _guard = self.write_lock.lock().await;
        self.persist
            .steerer()
            .set_steerer_enabled_preserving_config(id, enabled, &source_json)
            .await?;
        let rows = self.persist.steerer().list_steerers().await?;
        self.rebuild_arcswap(rows);
        self.republish(id).await;
        Ok(())
    }

    async fn republish(&self, steerer_id: i64) {
        if let Some(publisher) = &self.publisher {
            publisher
                .republish(BundleCause::steerer(steerer_id, Vec::new()))
                .await;
        }
    }

    pub fn bundle_publisher(&self) -> Option<Arc<BundlePublisher>> {
        self.publisher.clone()
    }

    pub async fn enrol_session(
        &self,
        session_uuid: uuid::Uuid,
        steerer_id: i64,
        mode_code: &str,
        authority_code: &str,
        enrolled_by: &str,
    ) -> Result<i64, SteeringServiceError> {
        self.persist
            .steerer().insert_enrolment(
                session_uuid,
                steerer_id,
                mode_code,
                authority_code,
                enrolled_by,
            )
            .await
            .map_err(|e| {
                tracing::error!(error = %e, session_id = %session_uuid, steerer_id, "enrolment insert failed");
                SteeringServiceError::Persistence(e)
            })
    }

    pub async fn disenrol_session(
        &self,
        enrolment_id: i64,
    ) -> Result<bool, SteeringServiceError> {
        self.persist
            .steerer()
            .delete_enrolment(enrolment_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, enrolment_id, "enrolment delete failed");
                SteeringServiceError::Persistence(e)
            })
    }

    fn rebuild_arcswap(&self, rows: Vec<crate::store::persistence::steering::SteererRow>) {
        let registry = SteererRegistry::from_rows(rows);
        self.registry.store(Arc::new(registry));
    }
}
