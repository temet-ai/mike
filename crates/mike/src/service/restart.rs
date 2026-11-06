use super::error::ServiceError;
use super::exec::{Context, activity, run_with_retry};
use super::files::{self, Existing};
use super::manager::Manager;
use super::spec::ServiceKind;
use super::state::Activity;
use crate::db_provision::Runner;

pub(crate) async fn restart<R: Runner>(ctx: &Context<'_, R>) -> Result<(), ServiceError> {
    for kind in ServiceKind::ALL {
        let path = ctx.manager.file_path(kind);
        match files::inspect(&path)? {
            Existing::Missing => {
                return Err(ServiceError::NotInstalled { name: kind.name() });
            }
            Existing::Foreign => return Err(ServiceError::ForeignFile { path }),
            Existing::Ours(_) => {}
        }
        let before = activity(ctx.runner, ctx.manager, kind).await?;
        for call in ctx.manager.restart_calls(kind, &before) {
            run_with_retry(ctx, &call).await?;
        }
        (ctx.out)(&format!(
            "{}: restarted under {} as {}",
            kind.name(),
            ctx.manager.platform.manager_name(),
            ctx.manager.identifier(kind)
        ));
    }
    Ok(())
}

pub(crate) fn plan(manager: &Manager) -> Vec<String> {
    let mut lines = vec!["Would run:".to_string()];
    for kind in ServiceKind::ALL {
        for call in manager.restart_calls(kind, &Activity::Running { pid: None }) {
            lines.push(format!("    {}", call.shown()));
        }
    }
    lines
}
