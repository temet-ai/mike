use super::error::ServiceError;
use super::exec::{Context, activity, run_with_retry};
use super::files::{self, Existing, Written};
use super::guard;
use super::manager::Manager;
use super::spec::{ServiceKind, Spec};
use super::state::Activity;
use crate::db_provision::Runner;

struct Staged<'a> {
    spec: &'a Spec,
    path: std::path::PathBuf,
    existing: Existing,
    content: String,
}

async fn current_activities<R: Runner>(
    ctx: &Context<'_, R>,
) -> Result<Vec<(ServiceKind, Activity)>, ServiceError> {
    let mut found = Vec::new();
    for kind in ServiceKind::ALL {
        found.push((kind, activity(ctx.runner, ctx.manager, kind).await?));
    }
    Ok(found)
}

fn stage<'a, R: Runner>(
    ctx: &Context<'_, R>,
    specs: &'a [Spec],
) -> Result<Vec<Staged<'a>>, ServiceError> {
    specs
        .iter()
        .map(|spec| {
            let path = ctx.manager.file_path(spec.kind);
            let existing = files::inspect(&path)?;
            if existing == Existing::Foreign {
                return Err(ServiceError::ForeignFile { path });
            }
            Ok(Staged {
                spec,
                content: ctx.manager.render(spec),
                path,
                existing,
            })
        })
        .collect()
}

fn create_log_dirs(specs: &[Spec]) -> Result<(), ServiceError> {
    for spec in specs {
        let dir = spec.log_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|source| ServiceError::Write { path: dir, source })?;
    }
    Ok(())
}

fn report_write<R: Runner>(ctx: &Context<'_, R>, staged: &Staged<'_>, written: Written) {
    let path = staged.path.display();
    let line = match written {
        Written::Created => format!("wrote {path}"),
        Written::Replaced => format!("updated {path}"),
        Written::Unchanged => format!("{path} is already up to date"),
    };
    (ctx.out)(&line);
}

async fn apply<R: Runner>(
    ctx: &Context<'_, R>,
    staged: &Staged<'_>,
    written: Written,
    before: &Activity,
) -> Result<(), ServiceError> {
    let kind = staged.spec.kind;
    let calls = ctx
        .manager
        .install_calls(kind, before, written != Written::Unchanged);
    for call in &calls {
        run_with_retry(ctx, call).await?;
    }
    let manager = ctx.manager.platform.manager_name();
    let identifier = ctx.manager.identifier(kind);
    let line = if calls.is_empty() {
        format!(
            "{}: already running under {manager} as {identifier}",
            kind.name()
        )
    } else {
        format!("{}: started under {manager} as {identifier}", kind.name())
    };
    (ctx.out)(&line);
    Ok(())
}

pub(crate) async fn install<R: Runner>(
    ctx: &Context<'_, R>,
    specs: &[Spec],
) -> Result<(), ServiceError> {
    let before = current_activities(ctx).await?;
    guard::refuse_conflicts(ctx, &before).await?;
    let staged = stage(ctx, specs)?;
    create_log_dirs(specs)?;
    let mut written = Vec::new();
    for item in &staged {
        let outcome = files::write(&item.path, &item.content, &item.existing)?;
        report_write(ctx, item, outcome);
        written.push(outcome);
    }
    for (item, outcome) in staged.iter().zip(written) {
        let was = before
            .iter()
            .find(|(kind, _)| *kind == item.spec.kind)
            .map_or(Activity::NotLoaded, |(_, activity)| activity.clone());
        apply(ctx, item, outcome, &was).await?;
    }
    Ok(())
}

pub(crate) fn plan(manager: &Manager, specs: &[Spec]) -> Vec<String> {
    let mut lines = Vec::new();
    for spec in specs {
        lines.push(format!(
            "Would write {}:",
            manager.file_path(spec.kind).display()
        ));
        lines.extend(
            manager
                .render(spec)
                .lines()
                .map(|line| format!("    {line}")),
        );
        lines.push(String::new());
    }
    lines.push("Would then run:".to_string());
    for spec in specs {
        for call in manager.install_calls(spec.kind, &Activity::NotLoaded, true) {
            lines.push(format!("    {}", call.shown()));
        }
    }
    lines
}
