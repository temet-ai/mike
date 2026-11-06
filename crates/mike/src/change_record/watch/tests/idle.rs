use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use super::{Db, db};
use crate::change_record::watch::{Checkout, Git, WatchConfig, start};

fn cpu_seconds(who: libc::c_int) -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let done = unsafe { libc::getrusage(who, usage.as_mut_ptr()) };
    if done != 0 {
        return 0.0;
    }
    let usage = unsafe { usage.assume_init() };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

fn checkout_root() -> String {
    std::env::var("MIKE_WATCH_IDLE_ROOT")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string())
}

#[tokio::test]
#[ignore = "measures a full minute of the watcher's processor use on a real checkout; run once by hand with --run-ignored"]
async fn the_watcher_is_cheap_while_the_checkout_is_idle() -> anyhow::Result<()> {
    let root = Git::new(checkout_root()).show_toplevel().await?;
    let Some(Db {
        db,
        persist,
        project_id,
    }) = db(&root).await?
    else {
        return Ok(());
    };
    let running = start(
        persist,
        Checkout {
            root: root.clone(),
            project_id,
        },
        WatchConfig::default(),
        CancellationToken::new(),
    );
    let opened = Instant::now();
    while !running.stats.ready.load(Ordering::Acquire) {
        anyhow::ensure!(!running.handle.is_finished(), "the watcher ended");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let baseline = opened.elapsed();
    let own = cpu_seconds(libc::RUSAGE_SELF);
    let children = cpu_seconds(libc::RUSAGE_CHILDREN);
    let window = Duration::from_secs(60);
    tokio::time::sleep(window).await;
    let used_own = cpu_seconds(libc::RUSAGE_SELF) - own;
    let used_children = cpu_seconds(libc::RUSAGE_CHILDREN) - children;
    let share = (used_own + used_children) / window.as_secs_f64() * 100.0;
    eprintln!(
        "idle watcher on {root}: baseline {:.2} s; over {} s: {used_own:.3} s in mike, {used_children:.3} s in git, {share:.3}% of one core; backstop polls {}, events {}, notifications {}",
        baseline.as_secs_f64(),
        window.as_secs(),
        running.stats.backstop_polls.load(Ordering::Acquire),
        running.stats.events_recorded.load(Ordering::Acquire),
        running.stats.notifications.load(Ordering::Acquire),
    );
    running.token.cancel();
    tokio::time::timeout(Duration::from_secs(10), running.handle).await??;
    db.cleanup().await
}
