use std::collections::BTreeSet;
use std::path::Path;

use uuid::Uuid;

use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::gate::Target;
use crate::store::persistence::PersistHandle;

pub async fn deliver(
    persist: &PersistHandle,
    def: &FleetDefinition,
    run: &str,
    commit: &str,
    result: (Option<i32>, &str, &Path),
) -> BTreeSet<Uuid> {
    let target = Target {
        commit: commit.into(),
        pushed: true,
        slot: def.repo.clone().into(),
    };
    super::routes::tell_owners(persist, def, run, &target, result).await
}
