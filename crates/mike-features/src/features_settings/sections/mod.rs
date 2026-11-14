pub mod compress;
pub mod reminders;
pub mod session_judge;
pub mod weasel_intercept;

pub use compress::CompressConfig;
pub use reminders::RemindersConfig;
pub use session_judge::SessionJudgeConfig;
pub use weasel_intercept::WeaselInterceptConfig;

use crate::config::ConfigSection;

impl ConfigSection for CompressConfig {
    const KEY: &'static str = "compress";
}

impl ConfigSection for RemindersConfig {
    const KEY: &'static str = "reminders";
}

impl ConfigSection for WeaselInterceptConfig {
    const KEY: &'static str = "weasel_intercept";
}

impl ConfigSection for SessionJudgeConfig {
    const KEY: &'static str = "session_judge";
}
