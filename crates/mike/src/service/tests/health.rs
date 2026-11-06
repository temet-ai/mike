use std::cell::RefCell;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::service::endpoints::Endpoints;
use crate::service::error::ServiceError;
use crate::service::health::{Timing, verify};

const QUICK: Timing = Timing {
    deadline: Duration::from_millis(300),
    interval: Duration::from_millis(20),
};

struct Server {
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(status: &'static str, body: String) -> anyhow::Result<Server> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut request = [0_u8; 2048];
            let _ = socket.read(&mut request).await;
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });
    Ok(Server { addr, task })
}

fn health_body(sha: &str) -> String {
    format!(r#"{{"build":{{"sha":"{sha}"}},"process":{{"uptime_seconds":3}}}}"#)
}

fn endpoints(inspect: &str, sidecar: &str) -> Endpoints {
    Endpoints {
        proxy_addr: "127.0.0.1:9988".to_string(),
        inspect_addr: inspect.to_string(),
        spa_addr: None,
        sidecar_url: sidecar.to_string(),
    }
}

#[tokio::test]
async fn a_proxy_of_this_build_and_an_answering_sidecar_pass_and_are_reported() -> anyhow::Result<()>
{
    let proxy = serve(
        "200 OK",
        health_body(crate::change_log::current_build().sha),
    )
    .await?;
    let sidecar = serve("400 Bad Request", String::new()).await?;
    let lines = RefCell::new(Vec::new());
    let out = |line: &str| lines.borrow_mut().push(line.to_string());

    verify(
        &endpoints(&proxy.addr.to_string(), &format!("http://{}", sidecar.addr)),
        QUICK,
        &out,
    )
    .await?;

    let said = lines.borrow().join("\n");
    assert!(
        said.contains(&format!("http://{}/api/health answered", proxy.addr)),
        "{said}"
    );
    assert!(said.contains("the build of this binary"), "{said}");
    assert!(said.contains("(HTTP 400)"), "{said}");
    Ok(())
}

#[tokio::test]
async fn a_proxy_running_another_build_fails_and_names_both_builds() -> anyhow::Result<()> {
    let proxy = serve("200 OK", health_body("0123456789abcdef")).await?;
    let sidecar = serve("200 OK", String::new()).await?;
    let result = verify(
        &endpoints(&proxy.addr.to_string(), &format!("http://{}", sidecar.addr)),
        QUICK,
        &|_: &str| {},
    )
    .await;

    let Err(ServiceError::Unhealthy(why)) = result else {
        anyhow::bail!("expected an unhealthy result, got {result:?}");
    };
    assert!(why.contains("runs build 0123456"), "{why}");
    assert!(why.contains("this binary is build"), "{why}");
    Ok(())
}

#[tokio::test]
async fn a_proxy_that_never_answers_fails_after_the_deadline() -> anyhow::Result<()> {
    let result = verify(
        &endpoints("127.0.0.1:1", "http://127.0.0.1:1"),
        QUICK,
        &|_: &str| {},
    )
    .await;
    let Err(ServiceError::Unhealthy(why)) = result else {
        anyhow::bail!("expected an unhealthy result, got {result:?}");
    };
    assert!(why.contains("the proxy did not become healthy"), "{why}");
    assert!(
        why.contains("nothing answered http://127.0.0.1:1/api/health"),
        "{why}"
    );
    Ok(())
}

#[tokio::test]
async fn a_healthy_proxy_with_a_silent_sidecar_fails_on_the_sidecar() -> anyhow::Result<()> {
    let proxy = serve(
        "200 OK",
        health_body(crate::change_log::current_build().sha),
    )
    .await?;
    let result = verify(
        &endpoints(&proxy.addr.to_string(), "http://127.0.0.1:1"),
        QUICK,
        &|_: &str| {},
    )
    .await;
    let Err(ServiceError::Unhealthy(why)) = result else {
        anyhow::bail!("expected an unhealthy result, got {result:?}");
    };
    assert!(why.contains("the sidecar did not answer"), "{why}");
    Ok(())
}
