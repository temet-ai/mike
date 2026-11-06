use serde_json::Value;

use super::resolve::Resolved;
use super::{ChangesError, ChangesQuery};
use crate::change_record::answers;
use crate::store::persistence::PersistHandle;

pub(super) async fn diff(
    persist: &PersistHandle,
    resolved: &Resolved,
    query: &ChangesQuery,
) -> Result<Value, ChangesError> {
    if query.since.is_some() || query.paths.is_some() {
        return Err(ChangesError::Invalid("diff uses commit, one path since your place, or scope=mine; since and paths are not supported".into()));
    }
    let limit = query
        .line_limit
        .unwrap_or(answers::DEFAULT_DIFF_LINES)
        .clamp(1, answers::MAX_DIFF_LINES);
    let path = query.path.as_deref().map(|path| resolved.relative(path));
    let answer = if let Some(commit) = query.commit.as_deref() {
        answers::commit_diff(&resolved.checkout, commit, path.as_deref(), limit).await
    } else if let Some(path) = query.path.as_deref() {
        answers::path_diff(
            persist,
            &resolved.caller()?,
            &resolved.relative(path),
            limit,
        )
        .await
    } else {
        if query.scope.as_deref().is_some_and(|scope| scope != "mine") {
            return Err(ChangesError::Invalid(
                "an uncommitted diff supports only scope=mine".into(),
            ));
        }
        answers::own_diff(persist, &resolved.caller()?, limit).await
    }
    .map_err(ChangesError::Invalid)?;
    serde_json::to_value(answer).map_err(|e| ChangesError::Invalid(e.to_string()))
}
