use std::io;

use tokio::io::{AsyncWrite, AsyncWriteExt};

pub async fn write_prompt<W>(stdin: &mut W, prompt: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    match stdin.write_all(prompt).await {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        outcome => outcome,
    }
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::process::Stdio;
    use std::task::{Context, Poll};

    use tokio::process::Command;

    use super::*;

    struct Failing(io::ErrorKind);

    impl AsyncWrite for Failing {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(self.0.into()))
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn write_prompt_to_a_child_that_already_exited_is_not_an_error() -> anyhow::Result<()> {
        let mut child = Command::new("sh")
            .args(["-c", "exit 7"])
            .stdin(Stdio::piped())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("no stdin"))?;
        let status = child.wait().await?;
        write_prompt(&mut stdin, &vec![b'x'; 1 << 20]).await?;
        assert_eq!(status.code(), Some(7));
        Ok(())
    }

    #[tokio::test]
    async fn write_prompt_reports_any_other_write_failure() {
        let outcome = write_prompt(&mut Failing(io::ErrorKind::PermissionDenied), b"x").await;
        assert_eq!(
            outcome.map_err(|e| e.kind()),
            Err(io::ErrorKind::PermissionDenied)
        );
    }
}
