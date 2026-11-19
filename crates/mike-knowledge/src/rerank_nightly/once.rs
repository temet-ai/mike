use std::sync::Mutex;

use chrono::{DateTime, Utc};

static FINISHED_WINDOW: Mutex<Option<DateTime<Utc>>> = Mutex::new(None);

pub fn finished(window: DateTime<Utc>) -> bool {
    FINISHED_WINDOW
        .lock()
        .map(|done| *done == Some(window))
        .unwrap_or(false)
}

pub fn mark_finished(window: DateTime<Utc>) {
    if let Ok(mut done) = FINISHED_WINDOW.lock() {
        *done = Some(window);
    }
}
