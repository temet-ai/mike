mod dto;
mod parse;

use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio::sync::Mutex;

pub use dto::{
    FilterRuleCreateInput, FilterRuleEnvelopeOutput, FilterRulePatchInput, OptionalField,
};
pub(crate) use parse::{parse_action, parse_kind};

use super::{
    FilterConfig, FilterOutcome, FilterRule, RuleSource, apply_request_filter,
    canonicalize_project_path, normalize_title,
};
use crate::config::FilterWriteGuard;
use crate::store::persistence::{PersistError, PersistHandle};

pub struct FilterService {
    filter_config: Arc<ArcSwap<FilterConfig>>,

    write_lock: Arc<Mutex<FilterWriteGuard>>,

    persist: PersistHandle,
}

#[derive(Debug, thiserror::Error)]
pub enum FilterServiceError {
    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Parse(String),

    #[error("no filter rule with that id")]
    NotFound,

    #[error("persistence error: {0}")]
    Persistence(#[from] PersistError),
}

impl FilterService {
    pub async fn reconcile_on_startup(
        filter_config: Arc<ArcSwap<FilterConfig>>,
        write_lock: Arc<Mutex<FilterWriteGuard>>,
        persist: PersistHandle,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Arc::new(Self {
            filter_config,
            write_lock,
            persist,
        });

        {
            let _guard = service.write_lock.lock().await;
            let db_rules = service.persist.list_filter_rules().await?;
            service.rebuild_arcswap(db_rules);
        }

        Ok(service)
    }

    pub async fn create(
        &self,
        body: FilterRuleCreateInput,
    ) -> Result<FilterRuleEnvelopeOutput, FilterServiceError> {
        let kind = parse_kind(&body.kind).map_err(FilterServiceError::Parse)?;
        let action = parse_action(&body.action).map_err(FilterServiceError::Parse)?;

        let mut rule = FilterRule {
            id: 0,
            uuid: uuid::Uuid::new_v4(),
            kind,
            value: body.value,
            enabled: body.enabled,
            source: RuleSource::Ui,
            updated_at: Utc::now(),
            sort_order: 0,
            action,
            text: body.text,
            trigger_token: body.trigger_token,
            inject_text: body.inject_text,
            project_path: body.project_path.as_deref().map(canonicalize_project_path),
            style_override: body.style_override,
            title: normalize_title(body.title),
        };
        rule.validate().map_err(FilterServiceError::Validation)?;

        {
            let _guard = self.write_lock.lock().await;

            let (minted_id, minted_sort_order) =
                self.persist.insert_filter_rule(&rule).await.map_err(|e| {
                    tracing::error!(error = %e, "filter rule insert failed");
                    FilterServiceError::Persistence(e)
                })?;
            rule.id = minted_id;
            rule.sort_order = minted_sort_order;

            let current = self.filter_config.load_full();
            let mut new_rules: Vec<FilterRule> = current.rules.to_vec();
            new_rules.push(rule.clone());
            self.rebuild_arcswap(new_rules);
        }

        let mut warnings = Vec::new();
        if let Some(w) = self.anchor_unreachable_warning(&rule).await {
            warnings.push(w);
        }

        Ok(FilterRuleEnvelopeOutput { rule, warnings })
    }

    pub async fn update(
        &self,
        id: i64,
        patch: FilterRulePatchInput,
    ) -> Result<FilterRuleEnvelopeOutput, FilterServiceError> {
        let updated = {
            let _guard = self.write_lock.lock().await;

            let current = self.filter_config.load_full();
            let Some(existing) = current.rules.iter().find(|r| r.id == id) else {
                return Err(FilterServiceError::NotFound);
            };
            let mut updated = existing.clone();
            apply_patch_fields(&mut updated, patch)?;
            updated.updated_at = Utc::now();
            updated.validate().map_err(FilterServiceError::Validation)?;

            match self.persist.update_filter_rule(&updated).await {
                Ok(true) => {}
                Ok(false) => return Err(FilterServiceError::NotFound),
                Err(e) => {
                    tracing::error!(error = %e, "filter rule update failed");
                    return Err(FilterServiceError::Persistence(e));
                }
            }

            let mut new_rules: Vec<FilterRule> = current
                .rules
                .iter()
                .filter(|r| r.id != id)
                .cloned()
                .collect();
            new_rules.push(updated.clone());
            self.rebuild_arcswap(new_rules);

            updated
        };

        let mut warnings = Vec::new();
        if let Some(w) = self.anchor_unreachable_warning(&updated).await {
            warnings.push(w);
        }

        Ok(FilterRuleEnvelopeOutput {
            rule: updated,
            warnings,
        })
    }

    pub async fn delete(&self, id: i64) -> Result<(), FilterServiceError> {
        let _guard = self.write_lock.lock().await;

        match self.persist.delete_filter_rule(id).await {
            Ok(true) => {}
            Ok(false) => return Err(FilterServiceError::NotFound),
            Err(e) => {
                tracing::error!(error = %e, "filter rule delete failed");
                return Err(FilterServiceError::Persistence(e));
            }
        }

        let db_rules = self.persist.list_filter_rules().await?;
        self.rebuild_arcswap(db_rules);

        Ok(())
    }

    fn rebuild_arcswap(&self, mut new_rules: Vec<FilterRule>) {
        new_rules.sort_by(|a, b| {
            a.sort_order
                .cmp(&b.sort_order)
                .then_with(|| a.id.cmp(&b.id))
        });
        let cfg = FilterConfig { rules: new_rules };
        self.filter_config.store(Arc::new(cfg));
    }

    async fn anchor_unreachable_warning(&self, rule: &FilterRule) -> Option<String> {
        let request_id = self.latest_anchor_probe_request_id(rule).await?;
        let body = self.anchor_probe_body(rule, request_id).await?;

        let probe_cfg = FilterConfig {
            rules: vec![rule.clone()],
        };
        let outcome = apply_request_filter(
            &body,
            &probe_cfg,
            rule.project_path.as_deref().unwrap_or(""),
        );
        let unreachable = matches!(outcome, FilterOutcome::Unchanged)
            && super::content_blocks::anchor_matches_any_block(rule, &body);
        if !unreachable {
            return None;
        }

        tracing::warn!(
            rule_id = rule.id,
            request_id,
            "filter rule anchor is present in captured traffic but sits where the engine \
             can never rewrite it (e.g. the protected global-cache prefix of system[]) — \
             the rule will not fire on this traffic shape"
        );
        Some(
            "anchor_unreachable: the anchor text is present in the latest captured primary \
             request but sits where the filter can never rewrite (e.g. the protected \
             global-cache prefix of system[]) — the rule will not fire on this traffic shape"
                .to_string(),
        )
    }

    async fn latest_anchor_probe_request_id(&self, rule: &FilterRule) -> Option<u64> {
        let request_id = match self
            .persist
            .fetch_latest_primary_first_request_id(rule.project_path.as_deref())
            .await
        {
            Ok(Some(id)) => id,
            Ok(None) => {
                log_no_primary_first_request(rule.id);
                return None;
            }
            Err(e) => {
                log_primary_first_lookup_failed(rule.id, &e);
                return None;
            }
        };
        u64::try_from(request_id).ok()
    }

    async fn anchor_probe_body(&self, rule: &FilterRule, request_id: u64) -> Option<bytes::Bytes> {
        match self.persist.fetch_request_body_by_id(request_id).await {
            Ok(Some(served)) => Some(served.bytes),
            Ok(None) => {
                log_probe_body_missing(rule.id, request_id);
                None
            }
            Err(e) => {
                log_probe_body_fetch_failed(rule.id, request_id, &e);
                None
            }
        }
    }
}

fn log_no_primary_first_request(rule_id: i64) {
    tracing::debug!(
        rule_id,
        "anchor-reachability check: no captured primary_first request to check against"
    );
}

fn log_primary_first_lookup_failed(rule_id: i64, error: &PersistError) {
    tracing::debug!(
        rule_id,
        error = %error,
        "anchor-reachability check: latest primary_first lookup failed"
    );
}

fn log_probe_body_missing(rule_id: i64, request_id: u64) {
    tracing::debug!(
        rule_id,
        request_id,
        "anchor-reachability check: representative request has no retrievable body"
    );
}

fn log_probe_body_fetch_failed(rule_id: i64, request_id: u64, error: &PersistError) {
    tracing::debug!(
        rule_id,
        request_id,
        error = %error,
        "anchor-reachability check: body fetch failed"
    );
}

fn apply_patch_fields(
    rule: &mut FilterRule,
    patch: FilterRulePatchInput,
) -> Result<(), FilterServiceError> {
    if let Some(k) = patch.kind.as_deref() {
        rule.kind = parse_kind(k).map_err(FilterServiceError::Parse)?;
    }
    if let Some(v) = patch.value {
        rule.value = v;
    }
    if let Some(a) = patch.action.as_deref() {
        rule.action = parse_action(a).map_err(FilterServiceError::Parse)?;
    }
    match patch.text {
        OptionalField::Absent => {}
        OptionalField::Null => rule.text = None,
        OptionalField::Some(t) => rule.text = Some(t),
    }
    match patch.trigger_token {
        OptionalField::Absent => {}
        OptionalField::Null => rule.trigger_token = None,
        OptionalField::Some(t) => rule.trigger_token = Some(t),
    }
    match patch.inject_text {
        OptionalField::Absent => {}
        OptionalField::Null => rule.inject_text = None,
        OptionalField::Some(t) => rule.inject_text = Some(t),
    }
    if let Some(e) = patch.enabled {
        rule.enabled = e;
    }
    match patch.project_path {
        OptionalField::Absent => {}
        OptionalField::Null => rule.project_path = None,
        OptionalField::Some(p) => rule.project_path = Some(canonicalize_project_path(&p)),
    }
    if let Some(s) = patch.sort_order {
        rule.sort_order = s;
    }
    if let Some(v) = patch.style_override {
        rule.style_override = v;
    }
    match patch.title {
        OptionalField::Absent => {}
        OptionalField::Null => rule.title = None,
        OptionalField::Some(t) => rule.title = normalize_title(Some(t)),
    }
    Ok(())
}
