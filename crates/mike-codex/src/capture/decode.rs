use async_compression::tokio::bufread::ZstdDecoder;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("unsupported capture content encoding: {0}")]
    UnsupportedEncoding(String),
    #[error("capture content decoding failed: {0}")]
    Io(#[from] std::io::Error),
}

pub async fn decode_body(
    encoding: &str,
    input: impl AsyncRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> Result<u64, DecodeError> {
    let mut input = BufReader::new(input);
    let bytes = match encoding.trim().to_ascii_lowercase().as_str() {
        "" | "identity" => tokio::io::copy(&mut input, &mut output).await?,
        "zstd" => {
            let mut decoder = ZstdDecoder::new(input);
            decoder.multiple_members(true);
            tokio::io::copy(&mut decoder, &mut output).await?
        }
        _ => return Err(DecodeError::UnsupportedEncoding(encoding.to_owned())),
    };
    output.flush().await?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
