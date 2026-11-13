pub const STDERR_CAP_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StderrCapture {
    pub text: String,
    pub truncated: bool,
}

impl StderrCapture {
    pub fn capture(raw: &str) -> Self {
        if raw.len() <= STDERR_CAP_BYTES {
            return Self {
                text: raw.to_owned(),
                truncated: false,
            };
        }
        let mut end = STDERR_CAP_BYTES;
        while !raw.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: raw[..end].to_owned(),
            truncated: true,
        }
    }

    pub fn from_bytes(raw: &[u8]) -> Self {
        Self::capture(&String::from_utf8_lossy(raw))
    }
}
