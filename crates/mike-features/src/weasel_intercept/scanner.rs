pub struct DeltaScanner {
    forbidden: Vec<String>,
    delta_acc: String,
    max_word_len: usize,
}

impl DeltaScanner {
    pub fn new(words: &[String]) -> Self {
        let forbidden: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
        let max_word_len = forbidden
            .iter()
            .map(|w| w.chars().count())
            .max()
            .unwrap_or(0);
        Self {
            forbidden,
            delta_acc: String::new(),
            max_word_len,
        }
    }

    pub fn push(&mut self, chunk: &str) -> Option<String> {
        if self.forbidden.is_empty() || chunk.is_empty() {
            self.delta_acc.push_str(chunk);
            return None;
        }

        let overlap_len = self.overlap_byte_len();
        let tail = &self.delta_acc[self.delta_acc.len() - overlap_len..];
        let window = format!("{}{}", tail.to_lowercase(), chunk.to_lowercase());

        self.delta_acc.push_str(chunk);

        self.forbidden
            .iter()
            .find(|word| window.contains(word.as_str()))
            .cloned()
    }

    pub fn reset(&mut self) {
        self.delta_acc.clear();
    }

    pub fn text(&self) -> &str {
        &self.delta_acc
    }

    pub fn hold_chars(&self) -> usize {
        self.max_word_len.saturating_sub(1)
    }

    fn overlap_byte_len(&self) -> usize {
        if self.max_word_len == 0 || self.delta_acc.is_empty() {
            return 0;
        }
        let target_chars = self.max_word_len.saturating_sub(1);
        if target_chars == 0 {
            return 0;
        }
        let s = &self.delta_acc;
        let mut char_count = 0usize;
        let mut byte_pos = s.len();
        for (idx, _ch) in s.char_indices().rev() {
            char_count += 1;
            byte_pos = idx;
            if char_count >= target_chars {
                break;
            }
        }
        if char_count < target_chars {
            s.len()
        } else {
            s.len() - byte_pos
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn empty_forbidden_list_never_fires() {
        let mut scanner = DeltaScanner::new(&[]);
        assert!(scanner.push("comprehensive analysis").is_none());
        assert!(scanner.push("straightforward").is_none());
    }

    #[test]
    fn single_chunk_hit() {
        let mut scanner = DeltaScanner::new(&["comprehensive".to_string()]);
        let hit = scanner.push("This is a comprehensive analysis.");
        assert_eq!(hit.as_deref(), Some("comprehensive"));
    }

    #[test]
    fn case_insensitive_match() {
        let mut scanner = DeltaScanner::new(&["comprehensive".to_string()]);
        let hit = scanner.push("A Comprehensive approach.");
        assert_eq!(hit.as_deref(), Some("comprehensive"));
    }

    #[test]
    fn span_chunk_boundary() {
        let mut scanner = DeltaScanner::new(&["comprehensive".to_string()]);
        let hit1 = scanner.push("A compre");
        assert!(hit1.is_none(), "should not fire until boundary crossed");
        let hit2 = scanner.push("hensive analysis");
        assert_eq!(hit2.as_deref(), Some("comprehensive"));
    }

    #[test]
    fn no_false_positive_on_partial_word() {
        let mut scanner = DeltaScanner::new(&["comprehensive".to_string()]);
        assert!(scanner.push("A compre chunk.").is_none());
        scanner.reset();
        assert!(scanner.push("Another clean response.").is_none());
    }

    #[test]
    fn reset_clears_accumulator() {
        let mut scanner = DeltaScanner::new(&["comprehensive".to_string()]);
        scanner.push("A compre");
        scanner.reset();
        let hit = scanner.push("hensive analysis");
        assert!(hit.is_none());
    }

    #[test]
    fn multi_word_forbidden_list_returns_first_match() {
        let words = vec!["robust".to_string(), "straightforward".to_string()];
        let mut scanner = DeltaScanner::new(&words);
        let hit = scanner.push("This is a robust and straightforward solution.");
        assert_eq!(hit.as_deref(), Some("robust"));
    }

    #[test]
    fn phrase_forbidden_word_with_spaces() {
        let mut scanner = DeltaScanner::new(&["out of scope".to_string()]);
        let hit = scanner.push("That feature is out of scope for now.");
        assert_eq!(hit.as_deref(), Some("out of scope"));
    }

    #[test]
    fn phrase_spanning_chunks() {
        let mut scanner = DeltaScanner::new(&["out of scope".to_string()]);
        assert!(scanner.push("That feature is out").is_none());
        let hit = scanner.push(" of scope for now.");
        assert_eq!(hit.as_deref(), Some("out of scope"));
    }

    #[test]
    fn clean_chunks_return_none() {
        let words = vec!["comprehensive".to_string(), "robust".to_string()];
        let mut scanner = DeltaScanner::new(&words);
        assert!(scanner.push("This is a clean response.").is_none());
        assert!(scanner.push("Nothing forbidden here.").is_none());
        assert!(scanner.push("All good.").is_none());
    }
}
