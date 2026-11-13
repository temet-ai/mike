use tokio::sync::Mutex;

use super::cli::AnthropicCli;
use super::error::AnthropicCliError;
use super::login::{LoginSession, LoginStatusDto, spawn_login};

pub struct LoginRegistry {
    slot: Mutex<Option<LoginSession>>,
}

impl LoginRegistry {
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
        }
    }

    pub async fn start(
        &self,
        cli: &AnthropicCli,
    ) -> Result<LoginStatusDto, AnthropicCliError> {
        let mut guard = self.slot.lock().await;
        if let Some(existing) = guard.as_ref()
            && !existing.is_terminal().await
        {
            return Err(AnthropicCliError::AlreadyRunning(existing.id.clone()));
        }
        let session = spawn_login(cli)?;
        let status = session.status().await;
        *guard = Some(session);
        Ok(status)
    }

    pub async fn current_status(&self) -> Option<LoginStatusDto> {
        let guard = self.slot.lock().await;
        match guard.as_ref() {
            Some(session) => Some(session.status().await),
            None => None,
        }
    }

    pub async fn status(&self, id: &str) -> Result<LoginStatusDto, AnthropicCliError> {
        let guard = self.slot.lock().await;
        match guard.as_ref() {
            Some(session) if session.id == id => Ok(session.status().await),
            _ => Err(AnthropicCliError::NotRunning),
        }
    }

    pub async fn submit_code(&self, id: &str, code: &str) -> Result<(), AnthropicCliError> {
        let guard = self.slot.lock().await;
        match guard.as_ref() {
            Some(session) if session.id == id => session.submit_code(code).await,
            _ => Err(AnthropicCliError::NotRunning),
        }
    }

    pub async fn cancel(&self, id: &str) -> Result<(), AnthropicCliError> {
        let guard = self.slot.lock().await;
        match guard.as_ref() {
            Some(session) if session.id == id => {
                session.cancel();
                Ok(())
            }
            _ => Err(AnthropicCliError::NotRunning),
        }
    }
}
