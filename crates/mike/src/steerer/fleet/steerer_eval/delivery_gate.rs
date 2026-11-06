use chrono::{Duration, Utc};
use tracing::debug;

use super::judgement::Abstention;
use super::record::EvaluationScope;
use crate::steerer::fleet::context::FleetCtx;
use crate::steerer::registry::Steerer;
use crate::steerer::rollout_params::{Requirement, SteererClass};
use crate::store::persistence::PersistHandle;

pub(in crate::steerer::fleet) fn binary_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

fn fold_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn within_interval(
    persist: &PersistHandle,
    steerer: &Steerer,
    scope: &EvaluationScope,
) -> bool {
    let secs = steerer.params.delivery.min_interval_secs;
    if secs == 0 {
        return false;
    }
    let window = Duration::seconds(i64::try_from(secs).unwrap_or(i64::MAX));
    match persist.last_fire_for(scope.session_db_id, steerer.id).await {
        Ok(Some(last)) => last.created_at > Utc::now() - window,
        Ok(None) => false,
        Err(e) => {
            debug!(steerer_id = steerer.id, err = %e, "steerer fleet: last-fire lookup failed — not debouncing");
            false
        }
    }
}

async fn advice_already_given(
    persist: &PersistHandle,
    steerer: &Steerer,
    scope: &EvaluationScope,
) -> bool {
    if steerer.params.delivery.class != SteererClass::Efficiency {
        return false;
    }
    let hours = i64::try_from(steerer.params.delivery.advice_memory_hours).unwrap_or(i64::MAX);
    let since = Utc::now() - Duration::hours(hours);
    matches!(
        persist
            .last_delivered_fire_since(scope.session_db_id, steerer.id, since)
            .await,
        Ok(Some(_))
    )
}

async fn requirement_met(
    persist: &PersistHandle,
    scope: &EvaluationScope,
    requirement: &Requirement,
) -> bool {
    match requirement {
        Requirement::Binary(name) => binary_on_path(name),
        Requirement::ToolNameContains(text) => {
            let Some(request_id) = scope.trigger_request_id else {
                return false;
            };
            let needle = text.to_lowercase();
            persist
                .declared_tool_names(request_id)
                .await
                .unwrap_or_default()
                .iter()
                .any(|name| name.to_lowercase().contains(&needle))
        }
    }
}

async fn actionable(persist: &PersistHandle, steerer: &Steerer, scope: &EvaluationScope) -> bool {
    let requires = &steerer.params.delivery.requires;
    if requires.is_empty() {
        return true;
    }
    for requirement in requires {
        if requirement_met(persist, scope, requirement).await {
            return true;
        }
    }
    false
}

pub(in crate::steerer::fleet) async fn delivery_gate(
    ctx: &FleetCtx,
    steerer: &Steerer,
    scope: Option<&EvaluationScope>,
) -> Option<Abstention> {
    if ctx.config.force_fire {
        return None;
    }
    let (Some(persist), Some(scope)) = (ctx.persist.as_ref(), scope) else {
        return None;
    };
    if within_interval(persist, steerer, scope).await {
        return Some(Abstention::DebouncedByTime);
    }
    if advice_already_given(persist, steerer, scope).await {
        return Some(Abstention::DebouncedByAdvice);
    }
    if !actionable(persist, steerer, scope).await {
        return Some(Abstention::Unactionable);
    }
    None
}

pub(in crate::steerer::fleet) async fn repeats_last_delivery(
    ctx: &FleetCtx,
    steerer: &Steerer,
    scope: Option<&EvaluationScope>,
    nudge: &str,
) -> bool {
    if ctx.config.force_fire || steerer.params.delivery.class != SteererClass::Correction {
        return false;
    }
    let (Some(persist), Some(scope)) = (ctx.persist.as_ref(), scope) else {
        return false;
    };
    match persist.last_fire_for(scope.session_db_id, steerer.id).await {
        Ok(Some(last)) if last.was_delivered() => last
            .payload_text
            .as_deref()
            .is_some_and(|text| fold_whitespace(text) == fold_whitespace(nudge)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{binary_on_path, fold_whitespace};

    #[test]
    fn whitespace_folding_ignores_line_breaks_and_runs_of_spaces() {
        assert_eq!(
            fold_whitespace("use  ast-grep\n for this"),
            "use ast-grep for this"
        );
    }

    #[test]
    fn a_missing_binary_is_not_on_path() {
        assert!(!binary_on_path("mike-binary-that-does-not-exist-anywhere"));
    }
}
