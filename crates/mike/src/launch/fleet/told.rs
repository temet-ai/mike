use std::collections::HashSet;

use crate::store::persistence::PersistHandle;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Told {
    keys: HashSet<String>,
    unavailable: bool,
}

impl Told {
    pub fn already(&self, what: &str) -> bool {
        self.unavailable || self.keys.contains(what)
    }
}

pub async fn load(persist: &PersistHandle, run: &str) -> Told {
    match persist.told_keys(run).await {
        Ok(keys) => Told {
            keys: keys.into_iter().collect(),
            unavailable: false,
        },
        Err(err) => {
            tracing::warn!(%run, error = %err, "fleet: could not read what was already said in this run; saying nothing this tick");
            Told {
                keys: HashSet::new(),
                unavailable: true,
            }
        }
    }
}

pub async fn already(persist: &PersistHandle, run: &str, what: &str) -> bool {
    match persist.told_keys(run).await {
        Ok(keys) => keys.iter().any(|k| k == what),
        Err(err) => {
            tracing::warn!(%run, %what, error = %err, "fleet: could not read what was already said in this run; treating it as said");
            true
        }
    }
}

pub async fn mark(persist: &PersistHandle, run: &str, what: &[String]) {
    if let Err(err) = persist.mark_told(run, what).await {
        tracing::warn!(%run, error = %err, "fleet: could not record what was said in this run; it may be said again");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_ledger_suppresses_all_notices() {
        let unavailable = Told {
            keys: HashSet::new(),
            unavailable: true,
        };
        assert!(unavailable.already("new-key"));
        let loaded = Told {
            keys: HashSet::from(["sent".into()]),
            unavailable: false,
        };
        assert!(loaded.already("sent"));
        assert!(!loaded.already("new-key"));
    }
}
