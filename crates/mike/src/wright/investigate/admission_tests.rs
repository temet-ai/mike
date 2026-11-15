use std::fs::File;

use super::admission::{Locks, load_over, lock_held, machine_busy};

#[test]
fn investigate_load_above_processor_count_is_busy() {
    assert!(load_over("8.50 3.00 2.00 2/900 1", 8).is_some());
    assert!(load_over("8.00 3.00 2.00 2/900 1", 8).is_none());
    assert!(load_over("0.10 0.10 0.10 1/100 1", 1).is_none());
    assert!(load_over("garbage", 4).is_none());
}

#[test]
fn investigate_held_lock_is_busy_and_free_lock_is_not() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let lock = tmp.path().join("deploy.lock");
    assert!(!lock_held(&lock));
    let holder = File::create(&lock)?;
    assert!(!lock_held(&lock));
    holder.lock()?;
    assert!(lock_held(&lock));
    let load = tmp.path().join("loadavg");
    std::fs::write(&load, "0.00 0.00 0.00 1/1 1")?;
    let busy = machine_busy(&Locks {
        loadavg: &load,
        deploy: &lock,
    });
    assert!(busy.is_some_and(|reason| reason.contains("deploy")));
    holder.unlock()?;
    assert!(!lock_held(&lock));
    Ok(())
}
