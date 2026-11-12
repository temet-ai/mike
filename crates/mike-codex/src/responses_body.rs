#[derive(Debug)]
pub struct BodyCapture {
    bytes: Vec<u8>,
    limit: usize,
    pub received: u64,
    pub refused: bool,
    pub complete: bool,
}

impl BodyCapture {
    pub fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            received: 0,
            refused: false,
            complete: false,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.received = self.received.saturating_add(chunk.len() as u64);
        if self.limit == 0 || self.refused {
            return;
        }
        if self.bytes.len().saturating_add(chunk.len()) > self.limit {
            self.bytes = Vec::new();
            self.refused = true;
            return;
        }
        self.bytes.extend_from_slice(chunk);
    }

    pub fn body(&self) -> Option<&[u8]> {
        (self.limit > 0 && !self.refused && self.complete).then_some(self.bytes.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::BodyCapture;

    #[test]
    fn refuses_whole_evidence_at_budget_and_keeps_counting() {
        let mut capture = BodyCapture::new(4);
        capture.push(b"abcd");
        capture.push(b"e");
        capture.push(b"f");
        capture.complete = true;
        assert!(capture.refused);
        assert_eq!(capture.received, 6);
        assert!(capture.bytes.is_empty());
        assert!(capture.body().is_none());
    }

    #[test]
    fn disabled_and_incomplete_captures_never_return_a_partial_body() {
        for limit in [0, 4] {
            let mut capture = BodyCapture::new(limit);
            capture.push(b"abc");
            assert!(capture.body().is_none());
            capture.complete = true;
            assert_eq!(capture.body(), (limit > 0).then_some(b"abc".as_slice()));
        }
    }
}
