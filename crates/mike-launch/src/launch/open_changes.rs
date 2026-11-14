use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use crate::store::persistence::PersistHandle;

pub type PathsFuture<'a> = Pin<Box<dyn Future<Output = Option<Vec<String>>> + Send + 'a>>;

pub trait OpenChanges: Send + Sync {
    fn paths<'a>(&'a self, persist: &'a PersistHandle, checkout: &'a str) -> PathsFuture<'a>;
}

static OPEN: OnceLock<Arc<dyn OpenChanges>> = OnceLock::new();

pub fn install(open: Arc<dyn OpenChanges>) {
    let _ = OPEN.set(open);
}

pub async fn open_change_paths(persist: &PersistHandle, checkout: &str) -> Option<Vec<String>> {
    OPEN.get()?.paths(persist, checkout).await
}
