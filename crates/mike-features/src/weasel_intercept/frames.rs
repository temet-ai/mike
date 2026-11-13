use bytes::{Bytes, BytesMut};

pub struct Frame {
    pub raw: Bytes,
    pub event: String,
    pub data: String,
}

#[derive(Default)]
pub struct SseFramer {
    pending: BytesMut,
    scanned: usize,
}

impl SseFramer {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Frame> {
        self.pending.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(end) = frame_end(&self.pending, self.scanned) {
            frames.push(parse_frame(self.pending.split_to(end).freeze()));
            self.scanned = 0;
        }
        self.scanned = self.pending.len().saturating_sub(2);
        frames
    }

    pub fn take_remainder(&mut self) -> Bytes {
        self.scanned = 0;
        self.pending.split().freeze()
    }
}

fn frame_end(buf: &[u8], from: usize) -> Option<usize> {
    (from..buf.len()).find_map(|at| {
        if buf[at] != b'\n' {
            return None;
        }
        match (buf.get(at + 1), buf.get(at + 2)) {
            (Some(b'\n'), _) => Some(at + 2),
            (Some(b'\r'), Some(b'\n')) => Some(at + 3),
            _ => None,
        }
    })
}

fn parse_frame(raw: Bytes) -> Frame {
    let mut event = String::new();
    let mut data = String::new();
    if let Ok(text) = std::str::from_utf8(&raw) {
        for line in text.lines() {
            if let Some(value) = field(line, "event") {
                event = value.to_owned();
            } else if let Some(value) = field(line, "data") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
        }
    }
    Frame { raw, event, data }
}

fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let value = line.strip_prefix(name)?.strip_prefix(':')?;
    Some(value.strip_prefix(' ').unwrap_or(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(frames: &[Frame]) -> Vec<(&str, &str)> {
        frames
            .iter()
            .map(|f| (f.event.as_str(), f.data.as_str()))
            .collect()
    }

    #[test]
    fn one_chunk_holding_two_events_yields_two_frames() {
        let mut framer = SseFramer::default();
        let frames = framer.push(b"event: a\ndata: {\"x\":1}\n\nevent: b\ndata: {}\n\n");
        assert_eq!(events(&frames), vec![("a", "{\"x\":1}"), ("b", "{}")]);
        assert!(framer.take_remainder().is_empty());
    }

    #[test]
    fn an_event_split_across_chunks_is_held_until_its_blank_line_arrives() {
        let mut framer = SseFramer::default();
        assert!(framer.push(b"event: a\nda").is_empty());
        assert!(framer.push(b"ta: {\"x\":1}\n").is_empty());
        let frames = framer.push(b"\nevent: b");
        assert_eq!(events(&frames), vec![("a", "{\"x\":1}")]);
        assert_eq!(framer.take_remainder().as_ref(), b"event: b");
    }

    #[test]
    fn the_frames_put_back_together_are_the_bytes_that_arrived() {
        let wire = b"event: a\ndata: 1\n\n: comment\n\nevent: b\r\ndata: 2\r\n\r\ntail";
        let mut framer = SseFramer::default();
        let mut rebuilt = Vec::new();
        for chunk in wire.chunks(3) {
            for frame in framer.push(chunk) {
                rebuilt.extend_from_slice(&frame.raw);
            }
        }
        rebuilt.extend_from_slice(&framer.take_remainder());
        assert_eq!(rebuilt, wire);
    }

    #[test]
    fn carriage_return_line_endings_are_read_like_plain_ones() {
        let mut framer = SseFramer::default();
        let frames = framer.push(b"event: a\r\ndata: 1\r\n\r\n");
        assert_eq!(events(&frames), vec![("a", "1")]);
    }

    #[test]
    fn several_data_lines_join_with_a_newline() {
        let mut framer = SseFramer::default();
        let frames = framer.push(b"event: a\ndata: one\ndata: two\n\n");
        assert_eq!(events(&frames), vec![("a", "one\ntwo")]);
    }
}
