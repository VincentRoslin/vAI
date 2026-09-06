//! Split streaming assistant text into **clauses** for TTS (ADR-0005) — so
//! synthesis can start before the whole message is generated and a barge-in
//! loses at most one clause.
//!
//! Fed `push(delta)` as LLM `TokenDelta`s arrive; emits complete clauses.
//! `flush()` returns the tail at stream end. Deliberately simple: a slightly
//! early split on an abbreviation ("Dr. Smith") just adds a short pause — it
//! never drops or reorders text, and the concatenation of every emitted clause
//! plus the flushed tail equals the input exactly (modulo trimming).

/// Shortest clause worth emitting on its own — below this, keep accumulating
/// (so "Yes." / "OK." merge into the next clause rather than becoming a
/// stand-alone synthesis).
const MIN_CHARS: usize = 12;
/// Force a flush at the last space before this, even with no punctuation.
const MAX_CHARS: usize = 240;

/// Clause-boundary punctuation.
const BOUNDARY: [char; 6] = ['.', '!', '?', '…', ';', ':'];

/// Accumulates streamed text and emits clause-sized pieces.
#[derive(Debug, Default)]
pub struct ClauseChunker {
    buf: String,
}

impl ClauseChunker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append `delta` and return every complete clause now available (in order).
    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        let mut out = Vec::new();
        while let Some(end) = self.next_split() {
            let clause = self.buf[..end].trim().to_owned();
            self.buf.drain(..end);
            if !clause.is_empty() {
                out.push(clause);
            }
        }
        out
    }

    /// The remaining buffered text (trimmed), if any. Clears the buffer.
    pub fn flush(&mut self) -> Option<String> {
        let tail = self.buf.trim().to_owned();
        self.buf.clear();
        (!tail.is_empty()).then_some(tail)
    }

    /// Byte offset just past the end of the first complete clause, or `None`.
    fn next_split(&self) -> Option<usize> {
        let bytes = self.buf.as_bytes();
        let chars: Vec<(usize, char)> = self.buf.char_indices().collect();

        // A hard newline is always a boundary (past a token, at least).
        for &(i, c) in &chars {
            if c == '\n' && self.buf[..i].trim().chars().count() >= 1 {
                return Some(i + 1);
            }
        }

        for (idx, &(_, c)) in chars.iter().enumerate() {
            if !BOUNDARY.contains(&c) {
                continue;
            }
            // `3.14` / `v2.0` — a dot between digits is not a boundary.
            let prev = idx.checked_sub(1).map(|j| chars[j].1);
            let next = chars.get(idx + 1).map(|n| n.1);
            if c == '.'
                && prev.is_some_and(|p| p.is_ascii_digit())
                && next.is_some_and(|n| n.is_ascii_digit())
            {
                continue;
            }
            // Consume a run of boundary chars ("...", "?!").
            let mut j = idx + 1;
            while chars.get(j).is_some_and(|n| BOUNDARY.contains(&n.1)) {
                j += 1;
            }
            let after = chars.get(j).map(|n| n.1);
            // A boundary splits only when a non-boundary char follows it — that
            // proves the sentence ended and the next began. A boundary at the
            // very end of the buffer waits for the next `push` / `flush` to
            // disambiguate ("Dr." vs "…lazy dog.").
            let terminates = after.is_some_and(char::is_whitespace)
                || after.is_some_and(|a| matches!(a, '"' | '\'' | ')' | ']' | '”' | '’'));
            if !terminates {
                continue;
            }
            // `idx + 1` chars precede the (end of the) boundary run.
            if idx + 1 >= MIN_CHARS {
                let end = chars.get(j).map_or(bytes.len(), |n| n.0);
                return Some(end);
            }
        }

        // No punctuation but the buffer is long — flush at the last space.
        if self.buf.chars().count() > MAX_CHARS {
            let cut = self
                .buf
                .char_indices()
                .take(MAX_CHARS)
                .filter(|(_, c)| c.is_whitespace())
                .last()
                .map_or(self.buf.len().min(MAX_CHARS * 4), |(i, _)| i + 1);
            return Some(cut.min(self.buf.len()));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(deltas: &[&str]) -> (Vec<String>, Option<String>) {
        let mut c = ClauseChunker::new();
        let mut clauses = Vec::new();
        for d in deltas {
            clauses.extend(c.push(d));
        }
        let tail = c.flush();
        (clauses, tail)
    }

    #[test]
    fn splits_on_sentence_boundaries() {
        let (clauses, tail) = all(&["Hello there, this is a test. ", "How are you doing today?"]);
        assert_eq!(clauses, vec!["Hello there, this is a test."]);
        assert_eq!(tail.as_deref(), Some("How are you doing today?"));
    }

    #[test]
    fn does_not_split_inside_a_number_or_abbrev_token() {
        let (clauses, tail) =
            all(&["The value of pi is about 3.14 which is a common approximation."]);
        // The `3.14` is not a boundary, and the trailing `.` is at end-of-buffer
        // so it waits for flush — one clause total, number intact.
        assert!(clauses.is_empty(), "no mid-stream split: {clauses:?}");
        assert!(tail.unwrap().contains("3.14"));
    }

    #[test]
    fn a_completed_sentence_mid_stream_is_emitted() {
        // The `.` is followed by more text → it splits without waiting.
        let (clauses, tail) = all(&["First sentence here. Second sentence follows on."]);
        assert_eq!(clauses, vec!["First sentence here."]);
        assert_eq!(tail.as_deref(), Some("Second sentence follows on."));
    }

    #[test]
    fn short_leading_fragment_merges_with_the_next_clause() {
        // "Yes." (4 chars) is below MIN_CHARS → it does not split on its own.
        let (clauses, tail) = all(&[
            "Yes. ",
            "And here is a much longer follow-up. ",
            "Then a bit more.",
        ]);
        // Whatever the split, "Yes." is never emitted alone.
        assert!(!clauses.iter().any(|c| c == "Yes."), "{clauses:?}");
        let joined = clauses
            .iter()
            .chain(tail.iter())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("Yes. And here is a much longer follow-up."));
    }

    #[test]
    fn newline_forces_a_boundary() {
        let (clauses, _) = all(&["first line\nsecond line continues here for a while"]);
        assert_eq!(clauses, vec!["first line"]);
    }

    #[test]
    fn ellipsis_is_one_boundary() {
        let (clauses, tail) = all(&[
            "Well, I was thinking about it... ",
            "then I changed my mind.",
        ]);
        assert_eq!(clauses.len(), 1);
        assert!(clauses[0].ends_with("..."));
        assert_eq!(tail.as_deref(), Some("then I changed my mind."));
    }

    #[test]
    fn force_flush_on_a_very_long_run() {
        let long = "word ".repeat(80); // 400 chars, no punctuation
        let (clauses, tail) = all(&[&long]);
        assert!(!clauses.is_empty(), "should force-flush");
        assert!(clauses[0].chars().count() <= MAX_CHARS + 8);
        // Nothing lost.
        let joined = clauses.join(" ") + " " + tail.as_deref().unwrap_or("");
        assert_eq!(joined.split_whitespace().count(), 80);
    }

    #[test]
    fn streaming_one_char_at_a_time_matches_whole_input() {
        let text = "The quick brown fox. It jumped over the lazy dog! Then it ran away quickly.";
        let mut c = ClauseChunker::new();
        let mut got = String::new();
        for ch in text.chars() {
            for clause in c.push(&ch.to_string()) {
                got.push_str(&clause);
                got.push(' ');
            }
        }
        if let Some(t) = c.flush() {
            got.push_str(&t);
        }
        assert_eq!(
            got.split_whitespace().collect::<Vec<_>>(),
            text.split_whitespace().collect::<Vec<_>>()
        );
    }
}
