use ts_rs::{Config, TS};

use crate::api::handlers::{
    DiagnosticDto, RequestDetail, RequestSummary, SessionSummary, SystemBlockProjection,
    SystemBlocksProjection,
};
use crate::store::models::{DiagnosticCategory, RequestStatus};
use crate::types::{RequestParams, TokenUsage, ToolCall};

pub fn export_bindings() -> Result<(), ts_rs::ExportError> {
    let cfg = Config::new().with_out_dir(env!("CARGO_MANIFEST_DIR"));

    macro_rules! export {
        ($($ty:ty),+ $(,)?) => {
            $( <$ty as TS>::export_all(&cfg)?; )+
        };
    }

    export!(
        crate::api::session_flow::dto::SessionFlowPage,
        crate::api::session_flow::dto::SessionFlowCursor,
        RequestSummary,
        SessionSummary,
        RequestDetail,
        RequestParams,
        TokenUsage,
        ToolCall,
        RequestStatus,
        DiagnosticDto,
        DiagnosticCategory,
        SystemBlocksProjection,
        SystemBlockProjection,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn export_bindings() -> Result<(), ts_rs::ExportError> {
        super::export_bindings()
    }
}
