use rmcp::model::{CallToolResult, ErrorData as RmcpError};
use serde_json::{Map, Value};

use super::envelope::{apply_projection, extract_select};
use super::{
    config_history, cross_session, diagnostics, dossier, eval_bundles, eval_labels, eval_metrics,
    eval_runs, filters, fleets, improvement, launches, mail, memory, prompts, requests, semantic,
    session_activity, session_notes, session_verdicts, sessions, steer_questions,
    steering_evaluations, text_search, time_spent, turn_verdicts, turns, turns_summary, workspace,
};
use crate::client::InspectClient;

pub(crate) async fn call_tool(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    let (arguments, selectors) = extract_select(arguments)?;
    let result = dispatch(client, name, arguments).await?;
    Ok(apply_projection(result, &selectors))
}

async fn dispatch(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if mail::owns(name) {
        return mail::call_tool(client, name, arguments).await;
    }
    if requests::owns(name) {
        return requests::call_tool(client, name, arguments).await;
    }
    if text_search::owns(name) {
        return text_search::call_tool(client, name, arguments).await;
    }
    if sessions::owns(name) {
        return sessions::call_tool(client, name, arguments).await;
    }
    if session_activity::owns(name) {
        return session_activity::call_tool(client, name, arguments).await;
    }
    if time_spent::owns(name) {
        return time_spent::call_tool(client, name, arguments).await;
    }
    if session_notes::owns(name) {
        return session_notes::call_tool(client, name, arguments).await;
    }
    if turns::owns(name) {
        return turns::call_tool(client, name, arguments).await;
    }
    if turns_summary::owns(name) {
        return turns_summary::call_tool(client, name, arguments).await;
    }
    if diagnostics::owns(name) {
        return diagnostics::call_tool(client, name, arguments).await;
    }
    if filters::owns(name) {
        return filters::call_tool(client, name, arguments).await;
    }
    if prompts::owns(name) {
        return prompts::call_tool(client, name, arguments).await;
    }
    dispatch_project_tools(client, name, arguments).await
}

async fn dispatch_project_tools(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if super::knowledge::owns(name) {
        return super::knowledge::call_tool(client, name, arguments).await;
    }
    if memory::owns(name) {
        return memory::call_tool(client, name, arguments).await;
    }
    if workspace::owns(name) {
        return workspace::call_tool(client, name, arguments).await;
    }
    if launches::owns(name) {
        return launches::call_tool(client, name, arguments).await;
    }
    if fleets::owns(name) {
        return fleets::call_tool(client, name, arguments).await;
    }
    if super::fleet_tree::owns(name) {
        return super::fleet_tree::call_tool(client, name, arguments).await;
    }
    if super::fleet_check::owns(name) {
        return super::fleet_check::call_tool(client, name, arguments).await;
    }
    if super::fleet_record::owns(name) {
        return super::fleet_record::call_tool(client, name, arguments).await;
    }
    if super::fleet_propose::owns(name) {
        return super::fleet_propose::call_tool(client, name, arguments).await;
    }
    if super::fleet_report::owns(name) {
        return super::fleet_report::call_tool(client, name, arguments).await;
    }
    if super::fleet_decide::owns(name) {
        return super::fleet_decide::call_tool(client, arguments).await;
    }
    if super::changes::owns(name) {
        return super::changes::call_tool(client, name, arguments).await;
    }
    if super::work_threads::owns(name) {
        return super::work_threads::call_tool(client, name, arguments).await;
    }
    dispatch_judgement_tools(client, name, arguments).await
}

async fn dispatch_judgement_tools(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if config_history::owns(name) {
        return config_history::call_tool(client, name, arguments).await;
    }
    if eval_bundles::owns(name) {
        return eval_bundles::call_tool(client, name, arguments).await;
    }
    if dossier::owns(name) {
        return dossier::call_tool(client, name, arguments).await;
    }
    if eval_labels::owns(name) {
        return eval_labels::call_tool(client, name, arguments).await;
    }
    if turn_verdicts::owns(name) {
        return turn_verdicts::call_tool(client, name, arguments).await;
    }
    if cross_session::owns(name) {
        return cross_session::call_tool(client, name, arguments).await;
    }
    if super::questions::owns(name) {
        return super::questions::call_tool(client, name, arguments).await;
    }
    if super::threads::owns(name) {
        return super::threads::call_tool(client, name, arguments).await;
    }
    if steer_questions::owns(name) {
        return steer_questions::call_tool(client, name, arguments).await;
    }
    if session_verdicts::owns(name) {
        return session_verdicts::call_tool(client, name, arguments).await;
    }
    if improvement::owns(name) {
        return improvement::call_tool(client, name, arguments).await;
    }
    dispatch_eval_tools(client, name, arguments).await
}

async fn dispatch_eval_tools(
    client: &InspectClient,
    name: &str,
    arguments: Option<Map<String, Value>>,
) -> Result<CallToolResult, RmcpError> {
    if eval_runs::owns(name) {
        return eval_runs::call_tool(client, name, arguments).await;
    }
    if eval_metrics::owns(name) {
        return eval_metrics::call_tool(client, name, arguments).await;
    }
    if steering_evaluations::owns(name) {
        return steering_evaluations::call_tool(client, name, arguments).await;
    }
    if semantic::owns(name) {
        return semantic::call_tool(client, name, arguments).await;
    }
    Err(RmcpError::invalid_params(
        format!("unknown tool: {name}"),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::call_tool;
    use crate::client::InspectClient;
    use crate::tools::TOOL_LIST_REQUESTS;

    async fn mock_client() -> (wiremock::MockServer, InspectClient) {
        let server = wiremock::MockServer::start().await;
        let client = InspectClient::new(server.uri());
        (server, client)
    }

    fn text_block(result: &rmcp::model::CallToolResult) -> anyhow::Result<String> {
        result
            .content
            .first()
            .and_then(|block| block.as_text())
            .map(|text| text.text.clone())
            .ok_or_else(|| anyhow::anyhow!("expected a text content block"))
    }

    #[tokio::test]
    async fn call_tool_with_select_projects_structured_content() -> anyhow::Result<()> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let (server, client) = mock_client().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "path": "/v1/messages", "status": 200},
                {"id": 2, "path": "/v1/messages", "status": 429}
            ])))
            .mount(&server)
            .await;

        let mut args = Map::new();
        args.insert(
            "select".to_string(),
            Value::Array(vec![Value::String("/requests/*/id".to_string())]),
        );
        let result = call_tool(&client, TOOL_LIST_REQUESTS, Some(args)).await?;

        let structured = result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow::anyhow!("expected structured_content"))?;
        assert_eq!(structured["requests"], json!([{"id": 1}, {"id": 2}]));
        assert_eq!(structured["select_applied"], json!(["/requests/*/id"]));
        assert!(structured.get("select_unmatched").is_none());
        assert!(structured.get("available_roots").is_none());
        assert_eq!(result.is_error, Some(false));
        assert!(text_block(&result)?.contains("select_applied"));
        Ok(())
    }

    #[tokio::test]
    async fn call_tool_with_unmatched_selector_reports_it_without_erroring() -> anyhow::Result<()> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let (server, client) = mock_client().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "path": "/v1/messages"}
            ])))
            .mount(&server)
            .await;

        let mut args = Map::new();
        args.insert(
            "select".to_string(),
            Value::Array(vec![Value::String("/requests/*/nonexistent".to_string())]),
        );
        let result = call_tool(&client, TOOL_LIST_REQUESTS, Some(args)).await?;

        let structured = result
            .structured_content
            .ok_or_else(|| anyhow::anyhow!("expected structured_content"))?;
        assert_eq!(
            structured["select_unmatched"],
            json!(["/requests/*/nonexistent"])
        );
        assert_eq!(
            structured["available_roots"],
            json!({"/requests/*/nonexistent": {"prefix": "/requests/*", "keys": ["id", "path"]}})
        );
        assert_eq!(result.is_error, Some(false));
        Ok(())
    }

    #[tokio::test]
    async fn call_tool_without_select_matches_pre_projection_shape() -> anyhow::Result<()> {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, ResponseTemplate};

        let (server, client) = mock_client().await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "path": "/v1/messages"}
            ])))
            .mount(&server)
            .await;

        let result = call_tool(&client, TOOL_LIST_REQUESTS, None).await?;
        let structured = result
            .structured_content
            .ok_or_else(|| anyhow::anyhow!("expected structured_content"))?;
        assert_eq!(
            structured,
            json!({"requests": [{"id": 1, "path": "/v1/messages"}]})
        );
        assert!(structured.get("select_applied").is_none());
        assert!(structured.get("select_unmatched").is_none());
        Ok(())
    }

    #[tokio::test]
    async fn call_tool_select_not_an_array_is_invalid_params() -> anyhow::Result<()> {
        let (_server, client) = mock_client().await;
        let mut args = Map::new();
        args.insert("select".to_string(), Value::String("/requests".to_string()));
        let err = call_tool(&client, TOOL_LIST_REQUESTS, Some(args))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected invalid_params error"))?;
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("must be an array"));
        Ok(())
    }

    #[tokio::test]
    async fn call_tool_select_non_string_element_is_invalid_params() -> anyhow::Result<()> {
        let (_server, client) = mock_client().await;
        let mut args = Map::new();
        args.insert("select".to_string(), Value::Array(vec![json!(1)]));
        let err = call_tool(&client, TOOL_LIST_REQUESTS, Some(args))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected invalid_params error"))?;
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("must be strings"));
        Ok(())
    }

    #[tokio::test]
    async fn call_tool_select_bad_pointer_is_invalid_params() -> anyhow::Result<()> {
        let (_server, client) = mock_client().await;
        let mut args = Map::new();
        args.insert(
            "select".to_string(),
            Value::Array(vec![Value::String("foo".to_string())]),
        );
        let err = call_tool(&client, TOOL_LIST_REQUESTS, Some(args))
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected invalid_params error"))?;
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("must be empty or start with"));
        Ok(())
    }
}
