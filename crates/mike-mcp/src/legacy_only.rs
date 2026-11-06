use std::future::Future;

use rmcp::model::{
    ClientJsonRpcMessage, ClientRequest, ErrorData, ProtocolVersion, ServerJsonRpcMessage,
};
use rmcp::service::RoleServer;
use rmcp::transport::Transport;

pub(crate) struct LegacyOnly<T> {
    inner: T,
    supported: &'static [ProtocolVersion],
}

impl<T> LegacyOnly<T> {
    pub(crate) fn new(inner: T, supported: &'static [ProtocolVersion]) -> Self {
        Self { inner, supported }
    }
}

fn refusal(
    message: &ClientJsonRpcMessage,
    supported: &[ProtocolVersion],
) -> Option<ServerJsonRpcMessage> {
    let ClientJsonRpcMessage::Request(asked) = message else {
        return None;
    };
    if !matches!(asked.request, ClientRequest::DiscoverRequest(_)) {
        return None;
    }
    Some(ServerJsonRpcMessage::error(
        ErrorData::unsupported_protocol_version(ProtocolVersion::V_2026_07_28, supported),
        Some(asked.id.clone()),
    ))
}

impl<T> Transport<RoleServer> for LegacyOnly<T>
where
    T: Transport<RoleServer>,
{
    type Error = T::Error;

    fn send(
        &mut self,
        item: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            let message = self.inner.receive().await?;
            let Some(refusal) = refusal(&message, self.supported) else {
                return Some(message);
            };
            if let Err(error) = self.inner.send(refusal).await {
                tracing::warn!(%error, "mike-mcp: could not answer the protocol probe");
                return None;
            }
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use super::*;

    const LEGACY: &[ProtocolVersion] =
        &[ProtocolVersion::V_2025_06_18, ProtocolVersion::V_2025_11_25];

    struct Scripted {
        incoming: VecDeque<ClientJsonRpcMessage>,
        sent: Arc<Mutex<Vec<ServerJsonRpcMessage>>>,
    }

    impl Transport<RoleServer> for Scripted {
        type Error = std::io::Error;

        fn send(
            &mut self,
            item: ServerJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            let sent = self.sent.clone();
            async move {
                sent.lock().await.push(item);
                Ok(())
            }
        }

        async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
            self.incoming.pop_front()
        }

        async fn close(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn probe() -> serde_json::Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientCapabilities": {}
        }}})
    }

    fn initialize() -> serde_json::Value {
        json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "probe", "version": "0"}
        }})
    }

    #[tokio::test]
    async fn the_protocol_probe_is_declined_and_never_reaches_the_server() -> anyhow::Result<()> {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut transport = LegacyOnly::new(
            Scripted {
                incoming: VecDeque::from([
                    serde_json::from_value(probe())?,
                    serde_json::from_value(initialize())?,
                ]),
                sent: sent.clone(),
            },
            LEGACY,
        );
        let first = transport.receive().await;
        assert!(
            matches!(&first, Some(ClientJsonRpcMessage::Request(r)) if matches!(r.request, ClientRequest::InitializeRequest(_))),
            "the first message the server sees is the legacy handshake, got {first:?}"
        );
        let answers = sent.lock().await;
        assert_eq!(answers.len(), 1, "one answer, to the probe");
        let wire = serde_json::to_value(&answers[0])?;
        assert_eq!(wire["id"], json!(1));
        assert_eq!(
            wire["error"]["data"]["supported"],
            json!(["2025-06-18", "2025-11-25"]),
            "the refusal names the revisions the client can fall back to"
        );
        drop(answers);
        assert!(transport.receive().await.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn every_other_message_passes_through_untouched() -> anyhow::Result<()> {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut transport = LegacyOnly::new(
            Scripted {
                incoming: VecDeque::from([serde_json::from_value(initialize())?]),
                sent: sent.clone(),
            },
            LEGACY,
        );
        assert!(transport.receive().await.is_some());
        assert!(sent.lock().await.is_empty());
        Ok(())
    }

    fn answer(
        answers: &std::collections::BTreeMap<i64, serde_json::Value>,
        id: i64,
    ) -> anyhow::Result<&serde_json::Value> {
        answers
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("request {id} was not answered"))
    }

    #[tokio::test]
    async fn a_client_that_probes_first_still_gets_the_tool_list() -> anyhow::Result<()> {
        use rmcp::ServiceExt;
        use rmcp::transport::IntoTransport;
        use rmcp::transport::async_rw::TransportAdapterAsyncRW;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let (client_side, server_side) = tokio::io::duplex(1 << 20);
        let server = crate::server::McpServer::new(
            crate::client::InspectClient::new("http://127.0.0.1:9".to_string()),
            crate::tools::Profile::Core,
            None,
        );
        let transport = LegacyOnly::new(
            IntoTransport::<RoleServer, std::io::Error, TransportAdapterAsyncRW>::into_transport(
                tokio::io::split(server_side),
            ),
            crate::server::CHANNEL_CAPABLE_PROTOCOL_VERSIONS,
        );
        let serving = tokio::spawn(async move { server.serve(transport).await });
        let (client_read, mut client_write) = tokio::io::split(client_side);
        let mut lines = BufReader::new(client_read).lines();
        for message in [
            probe(),
            initialize(),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {}}),
        ] {
            client_write
                .write_all(format!("{message}\n").as_bytes())
                .await?;
        }
        let mut answers = std::collections::BTreeMap::new();
        while answers.len() < 3 {
            let line = tokio::time::timeout(std::time::Duration::from_secs(10), lines.next_line())
                .await??
                .ok_or_else(|| anyhow::anyhow!("the server closed before it answered"))?;
            let reply: serde_json::Value = serde_json::from_str(&line)?;
            if let Some(id) = reply["id"].as_i64() {
                answers.insert(id, reply);
            }
        }
        assert!(
            answer(&answers, 1)?["error"].is_object(),
            "the probe is declined"
        );
        assert_eq!(
            answer(&answers, 2)?["result"]["protocolVersion"],
            json!("2025-11-25")
        );
        let listed = answer(&answers, 3)?;
        assert!(
            listed["result"]["tools"]
                .as_array()
                .is_some_and(|tools| !tools.is_empty()),
            "a request with no per-request metadata is served after the probe, got {listed}"
        );
        serving.abort();
        Ok(())
    }
}
