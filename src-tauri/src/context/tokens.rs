//! A cheap, deterministic token estimate for the context builder's budget.
//!
//! No tokenizer dependency (the real count is the model's business, not ours).
//! The estimate is intentionally a slight **over**-count so the builder's budget
//! — `context - response_reserve - margin` — never overflows the real window in
//! practice. Heuristic: BPE tokenisers average ~4 chars/token for English prose;
//! we take `max(chars / 4, words * 0.75)` and round up.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

/// Estimated token count for `text`. Monotonic in length; deterministic.
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    if chars == 0 {
        return 0;
    }
    let words = text.split_whitespace().count().max(1);
    let by_chars = (chars as f64 / 4.0).ceil();
    let by_words = (words as f64 * 0.75).ceil();
    by_chars.max(by_words).max(1.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero_nonempty_is_at_least_one() {
        assert_eq!(estimate_tokens(""), 0);
        assert!(estimate_tokens("hi") >= 1);
    }

    #[test]
    fn monotonic_and_deterministic() {
        let a = "The quick brown fox";
        let b = "The quick brown fox jumps over the lazy dog again and again";
        assert!(estimate_tokens(a) < estimate_tokens(b));
        assert_eq!(estimate_tokens(b), estimate_tokens(b));
    }

    #[test]
    fn roughly_chars_over_four_for_prose() {
        let s = "a".repeat(400); // 400 chars, 1 "word"
        let t = estimate_tokens(&s);
        assert!((90..=110).contains(&t), "got {t}");
    }
}
