use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::build_admission::{self, Pending, Permit};
use super::operation::ResourceClaim;

struct Counted {
    slots: Vec<Pending>,
    taken: Option<usize>,
}

impl Counted {
    fn try_take(&mut self, token: &CancellationToken) -> Result<bool, String> {
        for (at, slot) in self.slots.iter().enumerate() {
            if slot.try_take(token).map_err(|error| error.to_string())? {
                self.taken = Some(at);
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn release(&mut self) {
        if let Some(at) = self.taken.take() {
            self.slots[at].release();
        }
    }

    fn into_permit(mut self) -> Option<Permit> {
        let at = self.taken.take()?;
        Some(self.slots.swap_remove(at).into_permit())
    }
}

fn capacities<R: Into<ResourceClaim> + Clone>(resources: &[R]) -> BTreeMap<String, u32> {
    let mut named = BTreeMap::new();
    for claim in resources.iter().cloned().map(Into::into) {
        let capacity = claim.capacity().max(1);
        named
            .entry(claim.name().trim().to_string())
            .and_modify(|current: &mut u32| *current = (*current).min(capacity))
            .or_insert(capacity);
    }
    named
}

pub async fn acquire<R: Into<ResourceClaim> + Clone>(
    root: &Path,
    resources: &[R],
    token: &CancellationToken,
) -> Result<Permit, String> {
    let mut counted = Vec::new();
    for (name, capacity) in capacities(resources) {
        let directory = directory(root, &name)?;
        let slots = (0..capacity as usize)
            .map(|slot| Pending::open_slot(&directory, slot).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        counted.push(Counted { slots, taken: None });
    }
    loop {
        if take_all(&mut counted, token)? {
            if token.is_cancelled() {
                return Err("operation admission cancelled".into());
            }
            let permits = counted
                .into_iter()
                .filter_map(Counted::into_permit)
                .collect();
            return Ok(Permit::Resources { _permits: permits });
        }
        build_admission::pause(token)
            .await
            .map_err(|error| error.to_string())?;
    }
}

fn take_all(counted: &mut [Counted], token: &CancellationToken) -> Result<bool, String> {
    for at in 0..counted.len() {
        let taken = counted[at].try_take(token);
        if !matches!(taken, Ok(true)) {
            counted[..at].iter_mut().for_each(Counted::release);
            return taken;
        }
    }
    Ok(true)
}

pub fn directory(root: &Path, name: &str) -> Result<PathBuf, String> {
    let path = root.join(format!(
        "resource-{}",
        hex::encode(Sha256::digest(name.as_bytes()))
    ));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.to_string()),
    }
    Ok(path)
}

#[cfg(all(test, unix))]
#[path = "operation_admission_tests.rs"]
pub mod tests;
