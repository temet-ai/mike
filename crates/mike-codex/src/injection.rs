mod scan;

use std::io::{BufReader, Read, Seek, SeekFrom, Write};

use serde::Deserialize;
use serde::de::IgnoredAny;

use scan::Input;

const MESSAGE_START: &[u8] =
    br#"{"type":"message","role":"user","content":[{"type":"input_text","text":"#;
const MESSAGE_END: &[u8] = b"}]}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionOutcome {
    Injected { bytes_written: u64 },
    Unchanged(InjectionRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestInspection {
    Supported(RequestInfo),
    Unsupported(InjectionRefusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestInfo {
    pub input: InputShape,
    pub last_input_is_tool_result: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputShape {
    Array,
    String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionRefusal {
    EmptyContext,
    NotObject,
    MissingInput,
    UnsupportedInput,
    DuplicateInput,
}

impl InjectionRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmptyContext => "empty_context",
            Self::NotObject => "not_object",
            Self::MissingInput => "missing_input",
            Self::UnsupportedInput => "unsupported_input",
            Self::DuplicateInput => "duplicate_input",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InjectionError {
    #[error("invalid Responses request JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Responses request injection I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid Responses request string: {0}")]
    InvalidString(&'static str),
    #[error("invalid Responses request UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),
}

pub fn inspect_request(
    input: &mut (impl Read + Seek),
) -> Result<RequestInspection, InjectionError> {
    Ok(match plan(input)? {
        Ok(selected) => RequestInspection::Supported(selected.info()),
        Err(refusal) => RequestInspection::Unsupported(refusal),
    })
}

pub fn inject_context(
    input: &mut (impl Read + Seek),
    output: &mut impl Write,
    context: &str,
) -> Result<InjectionOutcome, InjectionError> {
    if context.trim().is_empty() {
        return Ok(InjectionOutcome::Unchanged(InjectionRefusal::EmptyContext));
    }
    let selected = match plan(input)? {
        Ok(selected) => selected,
        Err(refusal) => return Ok(InjectionOutcome::Unchanged(refusal)),
    };
    let mut output = CountedWriter { output, written: 0 };
    splice(input, &mut output, selected, context)?;
    output.flush()?;
    Ok(InjectionOutcome::Injected {
        bytes_written: output.written,
    })
}

fn plan(input: &mut (impl Read + Seek)) -> Result<Result<Input, InjectionRefusal>, InjectionError> {
    input.seek(SeekFrom::Start(0))?;
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(&mut *input));
    IgnoredAny::deserialize(&mut deserializer)?;
    deserializer.end()?;
    input.seek(SeekFrom::Start(0))?;
    scan::locate(input)
}

fn splice(
    input: &mut (impl Read + Seek),
    output: &mut impl Write,
    selected: Input,
    context: &str,
) -> Result<(), InjectionError> {
    let tail = match selected {
        Input::Array {
            close, nonempty, ..
        } => {
            copy_range(input, output, 0, close)?;
            if nonempty {
                output.write_all(b",")?;
            }
            write_message(output, context)?;
            close
        }
        Input::String { start, end } => {
            copy_range(input, output, 0, start)?;
            output.write_all(b"[")?;
            output.write_all(MESSAGE_START)?;
            copy_range(input, output, start, end - start)?;
            output.write_all(MESSAGE_END)?;
            output.write_all(b",")?;
            write_message(output, context)?;
            output.write_all(b"]")?;
            end
        }
    };
    input.seek(SeekFrom::Start(tail))?;
    std::io::copy(input, output)?;
    Ok(())
}

fn write_message(output: &mut impl Write, context: &str) -> Result<(), InjectionError> {
    output.write_all(MESSAGE_START)?;
    serde_json::to_writer(&mut *output, context)?;
    output.write_all(MESSAGE_END)?;
    Ok(())
}

fn copy_range(
    input: &mut (impl Read + Seek),
    output: &mut impl Write,
    start: u64,
    length: u64,
) -> Result<(), std::io::Error> {
    input.seek(SeekFrom::Start(start))?;
    if std::io::copy(&mut input.take(length), output)? != length {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "Responses request changed while its bytes were being copied",
        ));
    }
    Ok(())
}

struct CountedWriter<'a, W> {
    output: &'a mut W,
    written: u64,
}

impl<W: Write> Write for CountedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.output.write(bytes)?;
        self.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
#[path = "injection/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "injection/io_tests.rs"]
mod io_tests;
