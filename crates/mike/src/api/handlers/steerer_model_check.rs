use super::*;
use crate::llm::model_ref::{ModelRef, ModelRefError, Provider};

pub(crate) type ModelCheckError = (StatusCode, Json<SteererErrorBody>);

fn shape_error(model: &str, error: &ModelRefError) -> ModelCheckError {
    steerer_error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        &format!("`{model}` is not a usable model: {error}"),
    )
}

pub(crate) fn check_steerer_model_shape(model: &str) -> Result<Option<ModelRef>, ModelCheckError> {
    if model.trim().is_empty() {
        return Ok(None);
    }
    ModelRef::validate(model)
        .map(Some)
        .map_err(|error| shape_error(model, &error))
}

pub(crate) async fn check_steerer_model_listed(
    state: &AppState,
    model: &str,
) -> Result<(), ModelCheckError> {
    let Some(parsed) = check_steerer_model_shape(model)? else {
        return Ok(());
    };
    if parsed.provider != Provider::OpenRouter {
        return Ok(());
    }
    let models = state.model_catalogue().get().await.map_err(|e| {
        tracing::warn!(error = %e, "steerer model check: model catalogue fetch failed");
        steerer_error_response(
            StatusCode::BAD_GATEWAY,
            "failed to fetch the OpenRouter model catalogue to validate the requested model",
        )
    })?;
    let capable = models
        .iter()
        .find(|m| m.id == parsed.model)
        .is_some_and(|m| m.supports_structured_output);
    if capable {
        return Ok(());
    }
    Err(steerer_error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        &format!(
            "{} is not a known structured-output-capable OpenRouter model id",
            parsed.model
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_model_means_inherit_and_is_accepted() {
        assert!(matches!(check_steerer_model_shape(""), Ok(None)));
        assert!(matches!(check_steerer_model_shape("  "), Ok(None)));
    }

    #[test]
    fn every_provider_form_is_accepted() {
        for model in [
            "claude/sonnet",
            "codex/gpt-6-sol",
            "openrouter/openai/gpt-6-sol",
            "google/gemini-3.8-flash",
        ] {
            assert!(
                matches!(check_steerer_model_shape(model), Ok(Some(_))),
                "{model}"
            );
        }
    }

    #[test]
    fn a_bare_provider_prefix_is_refused_with_the_model_in_the_message() {
        for model in ["claude/", "codex/", "openrouter/"] {
            let refused = check_steerer_model_shape(model);
            let Err((status, Json(body))) = refused else {
                panic!("{model} must be refused");
            };
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(body.error.contains(model), "{}", body.error);
        }
    }
}
