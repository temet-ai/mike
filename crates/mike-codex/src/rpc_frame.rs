use tokio::io::{AsyncBufRead, AsyncBufReadExt};

const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub(crate) enum Frame {
    Data(Vec<u8>),
    Oversized,
    End,
}

pub(crate) async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> std::io::Result<Frame> {
    let mut bytes = Vec::new();
    let mut oversized = false;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(if oversized {
                Frame::Oversized
            } else if bytes.is_empty() {
                Frame::End
            } else {
                Frame::Data(bytes)
            });
        }
        let end = chunk.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(chunk.len(), |index| index + 1);
        if !oversized && bytes.len().saturating_add(count) > MAX_FRAME_BYTES {
            oversized = true;
            bytes.clear();
        }
        if !oversized {
            bytes.extend_from_slice(&chunk[..count]);
        }
        reader.consume(count);
        if end.is_some() {
            return Ok(if oversized {
                Frame::Oversized
            } else {
                Frame::Data(bytes)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn oversized_frame_is_discarded_then_next_frame_recovers() -> anyhow::Result<()> {
        let bytes = format!("{}\n{{}}\n", "x".repeat(MAX_FRAME_BYTES));
        let mut reader = std::io::Cursor::new(bytes);
        assert!(matches!(read_frame(&mut reader).await?, Frame::Oversized));
        match read_frame(&mut reader).await? {
            Frame::Data(bytes) => assert_eq!(bytes, b"{}\n"),
            _ => anyhow::bail!("next frame lost"),
        }
        Ok(())
    }
}
