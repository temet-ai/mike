pub(super) use super::test_support::{mk_finding, sample_request, user_turn_body};

use crate::extract::request::TurnOrigin;
use crate::feature_toggles::ResolvedToggles;
use crate::filter::FilterConfig;
use crate::lesson_worker::LessonSnapshot;
use crate::pipeline::StageContext;
use crate::store::models::RequestId;

pub(super) struct StageCtxFixture<'a> {
    pub(super) state: &'a crate::config::AppState,
    pub(super) session_toggles: &'a ResolvedToggles,
    pub(super) filter_cfg: &'a FilterConfig,
    pub(super) lesson_snapshot: &'a LessonSnapshot,
    pub(super) turn_origin: &'a TurnOrigin,
    pub(super) session_id: &'a str,
    pub(super) project_path: &'a str,
    pub(super) is_tool_loop: bool,
    pub(super) subagent_persona: Option<&'a str>,
}

pub(super) fn make_ctx(fixture: StageCtxFixture<'_>) -> StageContext<'_> {
    StageContext {
        state: fixture.state,
        session_toggles: fixture.session_toggles,
        filter_cfg: fixture.filter_cfg,
        lesson_snapshot: fixture.lesson_snapshot,
        session_id: fixture.session_id,
        project_path: fixture.project_path,
        turn_origin: fixture.turn_origin,
        request_id: RequestId::next(),
        is_tool_loop: fixture.is_tool_loop,
        subagent_persona: fixture.subagent_persona,
        has_agent_id: false,
    }
}
