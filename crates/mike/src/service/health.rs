use std::time::Duration;

use super::endpoints::Endpoints;
use super::error::ServiceError;
use crate::doctor::InspectView;
use crate::launch::cli::sidecar_reachable;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub(crate) deadline: Duration,
    pub(crate) interval: Duration,
}

impl Timing {
    pub(crate) const AFTER_START: Self = Self {
        deadline: Duration::from_secs(60),
        interval: Duration::from_millis(500),
    };
    pub(crate) const SNAPSHOT: Self = Self {
        deadline: Duration::from_secs(3),
        interval: Duration::from_millis(500),
    };
}

pub(crate) async fn verify(
    endpoints: &Endpoints,
    timing: Timing,
    out: &dyn Fn(&str),
) -> Result<(), ServiceError> {
    let build = crate::change_log::current_build();
    verify_proxy(endpoints, build.sha, build.sha_short, timing, out).await?;
    verify_sidecar(endpoints, timing, out).await
}

async fn verify_proxy(
    endpoints: &Endpoints,
    this_sha: &str,
    this_short: &str,
    timing: Timing,
    out: &dyn Fn(&str),
) -> Result<(), ServiceError> {
    let url = endpoints.health_url();
    let started = tokio::time::Instant::now();
    let mut last = format!("nothing answered {url}");
    loop {
        let view = InspectView::fetch(&endpoints.inspect_addr).await;
        match view.build_sha() {
            Some(sha) if sha == this_sha => {
                out(&format!(
                    "proxy: {url} answered and reports build {this_short}, the build of this binary"
                ));
                return Ok(());
            }
            Some(sha) => {
                last = format!(
                    "{url} answers, but it runs build {} and this binary is build {this_short}",
                    sha.get(..7).unwrap_or(&sha)
                );
            }
            None => {}
        }
        if started.elapsed() >= timing.deadline {
            return Err(ServiceError::Unhealthy(format!(
                "the proxy did not become healthy within {}s: {last}",
                timing.deadline.as_secs()
            )));
        }
        tokio::time::sleep(timing.interval).await;
    }
}

async fn verify_sidecar(
    endpoints: &Endpoints,
    timing: Timing,
    out: &dyn Fn(&str),
) -> Result<(), ServiceError> {
    let started = tokio::time::Instant::now();
    loop {
        match sidecar_reachable(&endpoints.sidecar_url).await {
            Ok(status) => {
                out(&format!(
                    "sidecar: {} answered (HTTP {status})",
                    endpoints.sidecar_url
                ));
                return Ok(());
            }
            Err(why) if started.elapsed() >= timing.deadline => {
                return Err(ServiceError::Unhealthy(format!(
                    "the sidecar did not answer within {}s: {why}",
                    timing.deadline.as_secs()
                )));
            }
            Err(_) => tokio::time::sleep(timing.interval).await,
        }
    }
}
