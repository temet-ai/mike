use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use super::{InjectionError, InjectionOutcome, inject_context};

#[test]
fn large_file_request_uses_fixed_io_windows_and_preserves_every_original_byte() -> anyhow::Result<()>
{
    let mut source = tempfile::tempfile()?;
    let prefix = br#"{"input":[{"role":"user","content":"#;
    source.write_all(prefix)?;
    source.write_all(b"\"")?;
    let chunk = [b'x'; 8192];
    for _ in 0..2048 {
        source.write_all(&chunk)?;
    }
    let suffix = b"\"}],\"future\":true}";
    source.write_all(suffix)?;
    let original_size = source.stream_position()?;
    source.rewind()?;
    let mut source = WindowedReader { source, reads: 0 };
    let mut output = tempfile::tempfile()?;
    let outcome = inject_context(&mut source, &mut output, "Mike external context")?;
    let output_size = output.stream_position()?;
    assert_eq!(
        outcome,
        InjectionOutcome::Injected {
            bytes_written: output_size
        }
    );
    assert!(output_size > original_size);
    assert!(source.reads > 2048);
    source.source.rewind()?;
    output.rewind()?;
    let unchanged_prefix = original_size - suffix.len() as u64 + 2;
    compare_bytes(&mut source.source, &mut output, unchanged_prefix)?;
    output.seek(SeekFrom::End(-((suffix.len() - 2) as i64)))?;
    let mut tail = vec![0; suffix.len() - 2];
    output.read_exact(&mut tail)?;
    assert_eq!(tail, suffix[2..]);
    Ok(())
}

fn compare_bytes(
    left: &mut impl Read,
    right: &mut impl Read,
    mut remaining: u64,
) -> anyhow::Result<()> {
    let mut left_buffer = [0; 8192];
    let mut right_buffer = [0; 8192];
    while remaining > 0 {
        let length = remaining.min(left_buffer.len() as u64) as usize;
        left.read_exact(&mut left_buffer[..length])?;
        right.read_exact(&mut right_buffer[..length])?;
        assert_eq!(left_buffer[..length], right_buffer[..length]);
        remaining -= length as u64;
    }
    Ok(())
}

struct WindowedReader<R> {
    source: R,
    reads: usize,
}

impl<R: Read> Read for WindowedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        assert!(buffer.len() <= 8192);
        self.reads += 1;
        self.source.read(buffer)
    }
}

impl<R: Seek> Seek for WindowedReader<R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.source.seek(position)
    }
}

#[test]
fn split_utf8_and_escape_bytes_across_io_windows_remain_unchanged() -> anyhow::Result<()> {
    let original = "{\"input\": [\"🦆\\u0070\\\"\"]}".as_bytes();
    let mut source = ShortReader(Cursor::new(original));
    let mut output = Vec::new();
    inject_context(&mut source, &mut output, "Mike context")?;
    let value: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(value["input"][0], "🦆p\"");
    assert!(output.starts_with(&original[..original.len() - 2]));
    Ok(())
}

struct ShortReader<R>(R);

impl<R: Read> Read for ShortReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let size = buffer.len().min(1);
        self.0.read(&mut buffer[..size])
    }
}

impl<R: Seek> Seek for ShortReader<R> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(position)
    }
}

#[test]
fn output_failure_propagates_and_keeps_source_untouched() {
    let original = b"{\"input\":[]}";
    let mut source = Cursor::new(original.to_vec());
    let result = inject_context(&mut source, &mut FailingWriter, "Mike context");
    assert!(matches!(result, Err(InjectionError::Io(_))));
    assert_eq!(source.into_inner(), original);
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("test output failure"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn source_shortened_after_validation_returns_error_without_output() {
    let mut source = ChangingReader {
        source: Cursor::new(br#"{"input":[]}"#.to_vec()),
        rewinds: 0,
    };
    let mut output = Vec::new();
    assert!(matches!(
        inject_context(&mut source, &mut output, "Mike context"),
        Err(InjectionError::Io(_))
    ));
    assert!(output.is_empty());
}

struct ChangingReader {
    source: Cursor<Vec<u8>>,
    rewinds: usize,
}

impl Read for ChangingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.source.read(buffer)
    }
}

impl Seek for ChangingReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        if position == SeekFrom::Start(0) {
            self.rewinds += 1;
            if self.rewinds == 2 {
                self.source = Cursor::new(br#"{"input":["#.to_vec());
            }
        }
        self.source.seek(position)
    }
}
