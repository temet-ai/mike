use std::any::Any;
use std::sync::{Arc, Mutex, Weak};

pub type Resource = Arc<dyn Any + Send + Sync>;

#[derive(Clone, Default)]
pub struct Resources(Arc<Mutex<Vec<Weak<dyn Any + Send + Sync>>>>);

impl Resources {
    pub fn retain<T: Any + Send + Sync>(&self, value: T) -> Arc<T> {
        let value = Arc::new(value);
        let erased: Resource = value.clone();
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::downgrade(&erased));
        value
    }

    pub fn snapshot(&self) -> Vec<Resource> {
        let mut resources = self.0.lock().unwrap_or_else(|error| error.into_inner());
        resources.retain(|resource| resource.strong_count() != 0);
        resources.iter().filter_map(Weak::upgrade).collect()
    }
}
