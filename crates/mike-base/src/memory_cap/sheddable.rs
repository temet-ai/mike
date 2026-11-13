use std::sync::Arc;

use crate::observability::shed::ShedPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShedPriority {
    Medium = 2,
}

#[derive(Debug, Clone)]
pub struct ShedReport {
    pub bytes_freed: u64,
    pub actions: u64,
    pub more_available: bool,
}

#[async_trait::async_trait]
pub trait Sheddable: Send + Sync + std::fmt::Debug {
    fn priority(&self) -> ShedPriority;

    async fn shed(&self, target_bytes: u64) -> Option<ShedReport>;

    fn shed_path(&self) -> ShedPath;
}

#[derive(Default)]
pub struct ShedRegistry {
    inner: Vec<Arc<dyn Sheddable>>,
}

pub fn priority_label(priority: ShedPriority) -> &'static str {
    match priority {
        ShedPriority::Medium => "medium",
    }
}

impl ShedRegistry {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn register(&mut self, sheddable: Arc<dyn Sheddable>) {
        self.inner.push(sheddable);
        self.inner.sort_by_key(|s| s.priority() as u8);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn Sheddable>> {
        self.inner.iter()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

impl std::fmt::Debug for ShedRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShedRegistry")
            .field("len", &self.inner.len())
            .finish()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeShed {
        priority: ShedPriority,
        path: ShedPath,
    }

    #[async_trait::async_trait]
    impl Sheddable for FakeShed {
        fn priority(&self) -> ShedPriority {
            self.priority
        }
        async fn shed(&self, _target_bytes: u64) -> Option<ShedReport> {
            Some(ShedReport {
                bytes_freed: 0,
                actions: 0,
                more_available: false,
            })
        }
        fn shed_path(&self) -> ShedPath {
            self.path
        }
    }

    #[test]
    fn registry_iter_returns_registered_shed_ables() {
        let mut reg = ShedRegistry::new();
        let m: Arc<dyn Sheddable> = Arc::new(FakeShed {
            priority: ShedPriority::Medium,
            path: ShedPath::StoreInsert,
        });
        reg.register(m);
        let priorities: Vec<ShedPriority> = reg.iter().map(|s| s.priority()).collect();
        assert_eq!(priorities, vec![ShedPriority::Medium]);
    }

    #[test]
    fn shed_priority_medium_round_trips() {
        let p = ShedPriority::Medium;
        assert_eq!(p, ShedPriority::Medium);
        assert_eq!(priority_label(p), "medium");
    }
}
