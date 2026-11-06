use super::error::ServiceError;
use super::exec::{Context, activity, run_checked};
use super::files::{self, Existing};
use super::manager::Manager;
use super::spec::ServiceKind;
use crate::db_provision::Runner;

const REMOVAL_ORDER: [ServiceKind; 2] = [ServiceKind::Sidecar, ServiceKind::Proxy];

pub(crate) async fn uninstall<R: Runner>(ctx: &Context<'_, R>) -> Result<(), ServiceError> {
    let mut removed_any = false;
    for kind in REMOVAL_ORDER {
        let path = ctx.manager.file_path(kind);
        match files::inspect(&path)? {
            Existing::Missing => (ctx.out)(&format!("{}: not installed", kind.name())),
            Existing::Foreign => (ctx.out)(&format!(
                "{}: left alone, because {} was not written by mike",
                kind.name(),
                path.display()
            )),
            Existing::Ours(_) => {
                unload(ctx, kind).await?;
                files::remove(&path)?;
                removed_any = true;
                (ctx.out)(&format!("{}: removed {}", kind.name(), path.display()));
            }
        }
    }
    if removed_any {
        for call in ctx.manager.after_removal_calls() {
            run_checked(ctx.runner, &call).await?;
        }
    }
    Ok(())
}

async fn unload<R: Runner>(ctx: &Context<'_, R>, kind: ServiceKind) -> Result<(), ServiceError> {
    let loaded = activity(ctx.runner, ctx.manager, kind).await?.is_loaded();
    if !loaded {
        return Ok(());
    }
    for call in ctx.manager.uninstall_calls(kind) {
        run_checked(ctx.runner, &call).await?;
    }
    Ok(())
}

pub(crate) fn plan(manager: &Manager) -> Vec<String> {
    let mut lines = Vec::new();
    for kind in REMOVAL_ORDER {
        lines.push(format!(
            "Would stop {} and remove {} if mike wrote it:",
            kind.name(),
            manager.file_path(kind).display()
        ));
        for call in manager.uninstall_calls(kind) {
            lines.push(format!("    {}", call.shown()));
        }
    }
    lines
}
