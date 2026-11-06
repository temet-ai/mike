use std::path::Path;

use url::Url;
use uuid::Uuid;

use super::DbError;
use crate::user_config::{ConfigFile, SettingValue};

const USER: &str = "mike";
const DATABASE: &str = "mike";
const URL_ENV_NAME: &str = "MIKE_DATABASE_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Managed {
    pub(crate) port: u16,
    pub(crate) password: String,
}

pub(crate) fn managed_from_url(url: &str) -> Option<Managed> {
    let parsed = Url::parse(url).ok()?;
    let ours = matches!(parsed.scheme(), "postgres" | "postgresql")
        && parsed.username() == USER
        && matches!(parsed.host_str(), Some("127.0.0.1" | "localhost"))
        && parsed.path() == format!("/{DATABASE}");
    if !ours {
        return None;
    }
    Some(Managed {
        port: parsed.port()?,
        password: parsed.password()?.to_string(),
    })
}

pub(crate) fn url_for(password: &str, port: u16) -> String {
    format!("postgres://{USER}:{password}@127.0.0.1:{port}/{DATABASE}")
}

pub(crate) fn generate_password() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(crate) fn stored_database_url(config_file: &Path) -> Result<Option<String>, DbError> {
    Ok(ConfigFile::open(config_file)?.literal(URL_ENV_NAME)?)
}

pub(crate) fn save_database_url(config_file: &Path, url: &str) -> Result<(), DbError> {
    let mut file = ConfigFile::open(config_file)?;
    file.set(URL_ENV_NAME, SettingValue::Text(url))?;
    file.set("MIKE_PERSIST_ENABLED", SettingValue::Bool(true))?;
    file.save()?;
    Ok(())
}
