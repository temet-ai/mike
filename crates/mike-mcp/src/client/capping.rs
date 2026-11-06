use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use reqwest::Response;
use serde_json::Value;

use super::InspectClient;
use crate::error::McpError;

impl InspectClient {
    pub(crate) async fn read_capped_json(
        &self,
        response: Response,
        endpoint_path: &str,
    ) -> Result<Value, McpError> {
        let bytes = self.read_capped_bytes(response, endpoint_path).await?;
        serde_json::from_slice::<Value>(&bytes).map_err(|e| McpError::Parse(e.to_string()))
    }

    pub(crate) async fn read_capped_bytes(
        &self,
        response: Response,
        endpoint_path: &str,
    ) -> Result<Bytes, McpError> {
        let limit = self.max_response_bytes;
        let advertised = response.content_length();
        if let Some(len) = advertised
            && len > limit
        {
            return Err(McpError::ResponseTooLarge {
                actual_bytes: Some(len),
                endpoint_path: endpoint_path.to_string(),
                limit_bytes: limit,
            });
        }

        let mut buf = BytesMut::with_capacity(advertised.unwrap_or(8192).min(64 * 1024) as usize);
        let mut total: u64 = 0;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                McpError::Unreachable(format!("body read for {endpoint_path} failed: {e}"))
            })?;
            total = total.saturating_add(chunk.len() as u64);
            if total > limit {
                return Err(McpError::ResponseTooLarge {
                    actual_bytes: advertised,
                    endpoint_path: endpoint_path.to_string(),
                    limit_bytes: limit,
                });
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf.freeze())
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    async fn start_capped_mock(cap: u64) -> (MockServer, InspectClient) {
        let server = MockServer::start().await;
        let client = InspectClient::with_max_response_bytes(server.uri(), cap);
        (server, client)
    }

    #[tokio::test]
    async fn body_under_cap_succeeds() -> anyhow::Result<()> {
        let (server, client) = start_capped_mock(8 * 1024).await;
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"id": 1, "path": "/v1/messages"}
            ])))
            .mount(&server)
            .await;

        let value = client.list_requests(None, None, None, None, None).await?;
        assert_eq!(value[0]["id"], 1);
        Ok(())
    }

    #[tokio::test]
    async fn body_exceeding_cap_streamed_returns_response_too_large() -> anyhow::Result<()> {
        let cap = 1024;
        let (server, client) = start_capped_mock(cap).await;
        let body = vec![b'x'; 5 * 1024];
        Mock::given(method("GET"))
            .and(path("/api/requests"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/json")
                    .set_body_bytes(body),
            )
            .mount(&server)
            .await;

        let err = client
            .list_requests(None, None, None, None, None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected ResponseTooLarge, got Ok"))?;
        match err {
            McpError::ResponseTooLarge {
                actual_bytes,
                endpoint_path,
                limit_bytes,
            } => {
                assert_eq!(limit_bytes, cap);
                assert_eq!(endpoint_path, "/api/requests");
                assert_eq!(actual_bytes, Some(5 * 1024));
            }
            other => anyhow::bail!("expected ResponseTooLarge, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn streaming_body_without_content_length_caps_during_read() -> anyhow::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let cap: u64 = 64 * 1024;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server_handle = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = [0u8; 4096];
            let _ = sock.read(&mut buf).await?;
            sock.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                      Transfer-Encoding: chunked\r\n\r\n",
            )
            .await?;
            let chunk = vec![b'x'; 32 * 1024];
            let chunk_header = format!("{:x}\r\n", chunk.len());
            for _ in 0..16 {
                sock.write_all(chunk_header.as_bytes()).await?;
                sock.write_all(&chunk).await?;
                sock.write_all(b"\r\n").await?;
                if sock.flush().await.is_err() {
                    break;
                }
            }
            let _ = sock.write_all(b"0\r\n\r\n").await;
            anyhow::Ok::<()>(())
        });

        let client = InspectClient::with_max_response_bytes(format!("http://{addr}"), cap);
        let err = client
            .list_requests(None, None, None, None, None)
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("expected ResponseTooLarge, got Ok"))?;
        match err {
            McpError::ResponseTooLarge {
                actual_bytes,
                endpoint_path,
                limit_bytes,
            } => {
                assert_eq!(limit_bytes, cap);
                assert_eq!(endpoint_path, "/api/requests");
                assert!(actual_bytes.is_none());
            }
            other => anyhow::bail!("expected ResponseTooLarge, got {other:?}"),
        }
        drop(server_handle);
        Ok(())
    }

    #[tokio::test]
    async fn default_client_cap_is_256_mib() {
        let client = InspectClient::new("http://127.0.0.1:9989".to_string());
        assert_eq!(client.max_response_bytes, 256 * 1024 * 1024);
    }
}
