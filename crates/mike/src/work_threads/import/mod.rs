mod frontmatter;
mod parse;
mod paths;
mod registry;
mod report;
mod sections;
mod sources;

#[cfg(test)]
mod parse_tests;

use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, NaiveDate, Utc};

use super::cli::ImportArgs;
use crate::config::CliOverrides;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::work_threads::ImportOutcome;
use parse::{ParseContext, Parsed, parse_worklog, registry_stub};
use registry::RegistryEntry;
use report::Report;
use sources::{ResolvedProject, WorklogFile};

pub(super) async fn run(args: ImportArgs, overrides: CliOverrides) -> anyhow::Result<()> {
    let config =
        crate::config::load(overrides).context("failed to resolve config for thread import")?;
    let url = config.database_url.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "thread import needs a database URL; set it in /settings or MIKE_DATABASE_URL"
        )
    })?;
    let handle = if args.dry_run {
        PersistHandle::connect_read_only(url).await
    } else {
        PersistHandle::connect_existing(url).await
    }
    .context("thread import could not connect to the database")?;
    let mut report = Report::default();
    let entries = read_registry(&args, &mut report)?;
    for repo in repo_list(&args, &entries) {
        let own: Vec<&RegistryEntry> = entries.iter().filter(|e| e.repo == repo).collect();
        import_repo(&handle, &repo, &own, args.dry_run, &mut report).await;
    }
    println!("{}", report.render(args.dry_run));
    Ok(())
}

fn read_registry(args: &ImportArgs, report: &mut Report) -> anyhow::Result<Vec<RegistryEntry>> {
    let path = match (&args.registry, registry::default_path()) {
        (Some(explicit), _) => Some(explicit.clone()),
        (None, Some(default)) if default.is_file() => Some(default),
        _ => None,
    };
    let Some(path) = path else {
        anyhow::ensure!(
            !args.repos.is_empty(),
            "no thread registry was found and no --repo was given; pass --registry <file> or --repo <path>"
        );
        return Ok(Vec::new());
    };
    let read = registry::read(&path)?;
    report.registry_problems = read.problems;
    Ok(read.entries)
}

fn repo_list(args: &ImportArgs, entries: &[RegistryEntry]) -> Vec<String> {
    let given = args.repos.iter().map(|repo| {
        let absolute = std::path::absolute(repo).unwrap_or_else(|_| repo.clone());
        absolute.to_string_lossy().trim_end_matches('/').to_owned()
    });
    let mut repos: Vec<String> = Vec::new();
    for repo in given.chain(entries.iter().map(|e| e.repo.clone())) {
        if !repos.contains(&repo) {
            repos.push(repo);
        }
    }
    repos
}

fn file_date(path: &Path) -> NaiveDate {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(|time| DateTime::<Utc>::from(time).date_naive())
        .unwrap_or_else(|_| Utc::now().date_naive())
}

fn read_one(
    file: &WorklogFile,
    project: &ResolvedProject,
    entry: Option<&RegistryEntry>,
) -> Parsed {
    let source_path = file.path.to_string_lossy().into_owned();
    let ctx = ParseContext {
        slug: &file.slug,
        source_path: &source_path,
        roots: &project.roots,
        registry: entry,
        file_date: file_date(&file.path),
    };
    match std::fs::read(&file.path) {
        Ok(bytes) => {
            let mut parsed = parse_worklog(&ctx, &String::from_utf8_lossy(&bytes));
            if std::str::from_utf8(&bytes).is_err() {
                parsed
                    .thread
                    .notes
                    .push("the file is not valid UTF-8; invalid bytes were replaced".to_owned());
            }
            parsed
        }
        Err(error) => registry_stub(
            &ctx,
            entry,
            Some(format!("the worklog could not be read: {error}")),
        ),
    }
}

fn collect(repo: &str, project: &ResolvedProject, entries: &[&RegistryEntry]) -> Vec<Parsed> {
    let files = sources::find_worklogs(Path::new(repo));
    let mut parsed: Vec<Parsed> = files
        .iter()
        .map(|file| {
            read_one(
                file,
                project,
                entries.iter().copied().find(|e| e.thread == file.slug),
            )
        })
        .collect();
    for entry in entries
        .iter()
        .filter(|e| !files.iter().any(|f| f.slug == e.thread))
    {
        let expected = format!("{repo}/artifacts/threads/{}/worklog.md", entry.thread);
        let ctx = ParseContext {
            slug: &entry.thread,
            source_path: &expected,
            roots: &project.roots,
            registry: Some(entry),
            file_date: Utc::now().date_naive(),
        };
        parsed.push(registry_stub(&ctx, Some(entry), None));
    }
    parsed
}

async fn import_repo(
    handle: &PersistHandle,
    repo: &str,
    entries: &[&RegistryEntry],
    dry_run: bool,
    report: &mut Report,
) {
    let project = match sources::resolve_repo(handle, repo).await {
        Ok(project) => project,
        Err(reason) => {
            report.skipped_repos.push((repo.to_owned(), reason));
            return;
        }
    };
    let parsed = collect(repo, &project, entries);
    for item in &parsed {
        apply_one(handle, &project, item, dry_run, report).await;
    }
    link_dependencies(handle, &project, &parsed, dry_run, report).await;
}

async fn apply_one(
    handle: &PersistHandle,
    project: &ResolvedProject,
    item: &Parsed,
    dry_run: bool,
    report: &mut Report,
) {
    let thread = &item.thread;
    let label = format!("{} [{}]", thread.slug, project.name);
    report.log_entries += thread.entries.len();
    if let Some(error) = &thread.parse_error {
        report
            .parse_errors
            .push((thread.source_path.clone(), error.clone()));
    }
    if dry_run {
        let exists = handle.work_thread_exists(project.id, &thread.slug).await;
        match exists {
            Ok(true) => report.updated.push(label),
            Ok(false) => report.created.push(label),
            Err(error) => report
                .left_alone
                .push((label, format!("could not be checked: {error}"))),
        }
        return;
    }
    match handle.import_work_thread(project.id, thread).await {
        Ok(ImportOutcome::Created) => report.created.push(label),
        Ok(ImportOutcome::Updated) => report.updated.push(label),
        Ok(ImportOutcome::LeftAlone(why)) => report.left_alone.push((label, why)),
        Err(error) => report
            .parse_errors
            .push((thread.source_path.clone(), format!("not stored: {error}"))),
    }
}

async fn link_dependencies(
    handle: &PersistHandle,
    project: &ResolvedProject,
    parsed: &[Parsed],
    dry_run: bool,
    report: &mut Report,
) {
    for item in parsed.iter().filter(|item| !item.depends_on.is_empty()) {
        let slug = &item.thread.slug;
        let unresolved = if dry_run {
            unknown_in_dry_run(handle, project, parsed, &item.depends_on).await
        } else {
            handle
                .link_imported_dependencies(project.id, slug, &item.depends_on)
                .await
                .unwrap_or_else(|error| vec![format!("(not linked: {error})")])
        };
        if !unresolved.is_empty() {
            report
                .unresolved_dependencies
                .push((format!("{slug} [{}]", project.name), unresolved));
        }
    }
}

async fn unknown_in_dry_run(
    handle: &PersistHandle,
    project: &ResolvedProject,
    parsed: &[Parsed],
    wanted: &[String],
) -> Vec<String> {
    let mut unknown = Vec::new();
    for slug in wanted {
        let in_batch = parsed.iter().any(|item| &item.thread.slug == slug);
        if !in_batch
            && !handle
                .work_thread_exists(project.id, slug)
                .await
                .unwrap_or(false)
        {
            unknown.push(slug.clone());
        }
    }
    unknown
}

#[cfg(test)]
mod import_tests;
