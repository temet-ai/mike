use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use tokio::net::{TcpListener, TcpStream, UnixStream};

const EXIT_RELAY_FAILED: i32 = 70;

pub(crate) async fn bind_all(addrs: &[SocketAddr]) -> Result<Vec<TcpListener>, String> {
    let mut listeners = Vec::with_capacity(addrs.len());
    for addr in addrs {
        listeners.push(
            TcpListener::bind(addr)
                .await
                .map_err(|e| format!("sandbox relay cannot listen on {addr}: {e}"))?,
        );
    }
    Ok(listeners)
}

async fn carry(mut tcp: TcpStream, socket: &Path) {
    match UnixStream::connect(socket).await {
        Ok(mut unix) => {
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut unix).await;
        }
        Err(e) => eprintln!(
            "sandbox relay cannot reach {}: {e}",
            socket.to_string_lossy()
        ),
    }
}

pub(crate) async fn pump(listener: TcpListener, socket: PathBuf) {
    loop {
        let Ok((tcp, _)) = listener.accept().await else {
            continue;
        };
        let socket = socket.clone();
        tokio::spawn(async move { carry(tcp, &socket).await });
    }
}

pub(crate) async fn run(socket: PathBuf, listen: Vec<SocketAddr>, command: Vec<OsString>) -> i32 {
    let Some((program, args)) = command.split_first() else {
        eprintln!("sandbox relay needs a command after `--`");
        return EXIT_RELAY_FAILED;
    };
    let listeners = match bind_all(&listen).await {
        Ok(listeners) => listeners,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_RELAY_FAILED;
        }
    };
    for listener in listeners {
        tokio::spawn(pump(listener, socket.clone()));
    }
    match tokio::process::Command::new(program)
        .args(args)
        .status()
        .await
    {
        Ok(status) => status.code().unwrap_or(EXIT_RELAY_FAILED),
        Err(e) => {
            eprintln!(
                "sandbox relay cannot start {}: {e}",
                program.to_string_lossy()
            );
            EXIT_RELAY_FAILED
        }
    }
}
