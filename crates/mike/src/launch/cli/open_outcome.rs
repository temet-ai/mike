use std::path::Path;

use uuid::Uuid;

use super::channel::{self, ChannelSource};
use super::delivery::Delivery;
use crate::launch::{LaunchError, LaunchOutput, LaunchSpec, Surface, channel_link};
use crate::store::persistence::launch::{LaunchOutcome, LaunchSurfaceRefs, StderrCapture};

pub(super) fn channel_source(folder: &Path, session: Option<Uuid>) -> ChannelSource {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty());
    ChannelSource {
        transcript: match (home, session) {
            (Some(home), Some(id)) => {
                Some(channel_link::transcript_path(Path::new(&home), folder, id))
            }
            _ => None,
        },
        cwd: folder.to_string_lossy().into_owned(),
    }
}

pub(super) fn surface_refs(surface: &Surface) -> LaunchSurfaceRefs {
    LaunchSurfaceRefs {
        workspace_ref: surface.workspace_ref.clone(),
        tab_ref: surface.tab_ref.clone(),
        surface_ref: Some(surface.surface_ref.clone()),
        pid: surface.pid.and_then(|pid| i32::try_from(pid).ok()),
        actual_placement: Some(surface.actual_placement.clone()),
    }
}

pub(super) fn opened_output(
    skeleton: LaunchOutput,
    spec: &LaunchSpec,
    surface: &Surface,
    delivery: &Delivery,
) -> LaunchOutput {
    let (delivered, confirmed) = match (&delivery.prompt, &spec.prompt) {
        (Some(_), _) => (surface.delivered.or(Some(false)), surface.confirmed),
        (None, Some(_)) => (Some(true), true),
        (None, None) => (None, false),
    };
    LaunchOutput {
        workspace: surface.workspace_ref.clone(),
        surface: Some(surface.surface_ref.clone()),
        session: spec.session,
        reused: surface.reused,
        opened: true,
        delivered,
        confirmed,
        brief_mode: spec.brief.as_ref().map(|b| b.mode),
        ..skeleton
    }
}

pub(super) fn failed_outcome(reason: &str, error: &LaunchError) -> LaunchOutcome {
    LaunchOutcome {
        result: "failed".to_string(),
        reason: Some(reason.to_string()),
        opened: false,
        delivered: None,
        confirmed: false,
        channel_check: None,
        stderr: Some(StderrCapture::capture(&error.to_string())),
    }
}

pub(super) fn success_outcome(output: &LaunchOutput, unconfirmed: bool) -> LaunchOutcome {
    let result = if unconfirmed {
        "unconfirmed"
    } else if output.reason.is_some() {
        "failed"
    } else {
        "success"
    };
    LaunchOutcome {
        result: result.to_string(),
        reason: output.reason.clone(),
        opened: output.opened,
        delivered: output.delivered,
        confirmed: output.confirmed,
        channel_check: Some(channel::as_str(output.channel_check).to_string()),
        stderr: None,
    }
}
