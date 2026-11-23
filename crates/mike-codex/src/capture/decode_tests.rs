use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite};

use super::{DecodeError, decode_body};

const ENCODED: &[u8] = include_bytes!("fixtures/large.zst");
const DECODED_BYTES: usize = 256 * 4097;

fn decoded() -> Vec<u8> {
    (0..DECODED_BYTES).map(|index| index as u8).collect()
}

#[tokio::test]
async fn identity_and_empty_encoding_copy_binary_content_unchanged() -> anyhow::Result<()> {
    let input = b"\0\xff\xfe unknown JSON fields or arbitrary bytes";
    for encoding in ["", "identity", " IDENTITY "] {
        let mut output = Vec::new();
        let bytes = decode_body(encoding, input.as_slice(), &mut output).await?;
        assert_eq!(bytes, input.len() as u64);
        assert_eq!(output, input);
    }
    let mut output = Vec::new();
    assert_eq!(decode_body("", &[][..], &mut output).await?, 0);
    assert!(output.is_empty());
    Ok(())
}

#[tokio::test]
async fn zstd_large_decoded_body_streams_into_a_file_without_a_content_limit() -> anyhow::Result<()>
{
    let mut output = tokio::fs::File::from_std(tempfile::tempfile()?);
    let bytes = decode_body("ZsTd", ENCODED, &mut output).await?;
    assert_eq!(bytes, DECODED_BYTES as u64);
    output.rewind().await?;
    let mut read_back = Vec::new();
    output.read_to_end(&mut read_back).await?;
    assert_eq!(read_back, decoded());
    Ok(())
}

#[tokio::test]
async fn empty_zstd_frame_decodes_successfully_without_inventing_content() -> anyhow::Result<()> {
    let encoded = [40, 181, 47, 253, 32, 0, 1, 0, 0];
    let mut output = Vec::new();
    assert_eq!(
        decode_body("zstd", encoded.as_slice(), &mut output).await?,
        0
    );
    assert!(output.is_empty());
    Ok(())
}

#[tokio::test]
async fn concatenated_zstd_frames_decode_all_members() -> anyhow::Result<()> {
    let input = [ENCODED, ENCODED].concat();
    let mut output = Vec::new();
    let bytes = decode_body("zstd", input.as_slice(), &mut output).await?;
    assert_eq!(bytes, 2 * DECODED_BYTES as u64);
    assert_eq!(output, decoded().repeat(2));
    Ok(())
}

#[tokio::test]
async fn malformed_truncated_or_trailing_zstd_bytes_report_failure() {
    let trailing = [ENCODED, b"trailing bytes"].concat();
    let truncated_member = [ENCODED, &ENCODED[..ENCODED.len() - 1]].concat();
    for input in [
        &[][..],
        b"not zstd".as_slice(),
        &ENCODED[..ENCODED.len() - 1],
        trailing.as_slice(),
        truncated_member.as_slice(),
    ] {
        let result = decode_body("zstd", input, tokio::io::sink()).await;
        assert!(matches!(result, Err(DecodeError::Io(_))));
    }
}

#[tokio::test]
async fn unsupported_encoding_leaves_destination_untouched() {
    for encoding in ["gzip", "br", "zstd, identity"] {
        let mut output = Vec::new();
        let result = decode_body(encoding, ENCODED, &mut output).await;
        assert!(matches!(result, Err(DecodeError::UnsupportedEncoding(_))));
        assert!(output.is_empty());
    }
}

struct RefusingOutput {
    reject_flush: bool,
}

impl AsyncWrite for RefusingOutput {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(if self.reject_flush {
            Ok(bytes.len())
        } else {
            Err(io::Error::other("output unavailable"))
        })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::other("flush unavailable")))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

#[tokio::test]
async fn output_write_or_flush_failure_is_not_reported_as_complete() {
    for reject_flush in [false, true] {
        let result = decode_body("zstd", ENCODED, RefusingOutput { reject_flush }).await;
        assert!(matches!(result, Err(DecodeError::Io(_))));
    }
}
