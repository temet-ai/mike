use std::time::Duration;

use reqwest::{Method, Url};
use serde_json::Value;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum FleetError {
    #[error("could not reach the inspect API at {addr}: {source}")]
    Unreachable {
        addr: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{0}")]
    Refused(String),
    #[error("the inspect API at {addr} answered something mike could not read: {source}")]
    Unreadable {
        addr: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the inspect address `{addr}` is not a usable address")]
    BadAddress { addr: String },
}

pub struct Api {
    addr: String,
    client: reqwest::Client,
}

impl Api {
    pub fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn url_at(&self, path: &str) -> Result<Url, FleetError> {
        Url::parse(&format!("http://{}{path}", self.addr)).map_err(|_| FleetError::BadAddress {
            addr: self.addr.clone(),
        })
    }

    pub fn url(&self, segments: &[&str]) -> Result<Url, FleetError> {
        let bad = || FleetError::BadAddress {
            addr: self.addr.clone(),
        };
        let mut url = Url::parse(&format!("http://{}/api/fleets", self.addr)).map_err(|_| bad())?;
        url.path_segments_mut()
            .map_err(|()| bad())?
            .extend(segments);
        Ok(url)
    }

    pub async fn get(&self, url: Url) -> Result<String, FleetError> {
        self.send(Method::GET, url, None, REQUEST_TIMEOUT).await
    }

    pub async fn post(&self, url: Url) -> Result<String, FleetError> {
        self.send(Method::POST, url, None, REQUEST_TIMEOUT).await
    }

    pub async fn post_json_and_wait(
        &self,
        url: Url,
        body: &Value,
        timeout: Duration,
    ) -> Result<String, FleetError> {
        self.send(Method::POST, url, Some(body), timeout).await
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<String, FleetError> {
        let unreachable = |source: reqwest::Error| FleetError::Unreachable {
            addr: self.addr.clone(),
            source,
        };
        let mut request = self.client.request(method, url).timeout(timeout);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(unreachable)?;
        let status = response.status();
        let body = response.text().await.map_err(unreachable)?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(FleetError::Refused(refusal_message(status.as_u16(), &body)))
        }
    }

    pub fn parse<T: serde::de::DeserializeOwned>(&self, body: &str) -> Result<T, FleetError> {
        serde_json::from_str(body).map_err(|source| FleetError::Unreadable {
            addr: self.addr.clone(),
            source,
        })
    }
}

pub fn refusal_message(status: u16, body: &str) -> String {
    let named = serde_json::from_str::<Value>(body).ok().and_then(|value| {
        ["message", "detail", "error"]
            .iter()
            .find_map(|key| value.get(*key).and_then(Value::as_str).map(str::to_string))
    });
    match named {
        Some(message) => message,
        None if body.trim().is_empty() => format!("the inspect API answered {status}"),
        None => body.trim().to_string(),
    }
}
