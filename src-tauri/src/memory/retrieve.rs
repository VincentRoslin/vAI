//! Turning a conversation turn into a deterministic FTS5 keyword query.
//!
//! `fts_query_for` lower-cases, splits on non-alphanumerics, drops a small
//! stopword set and very short tokens, de-duplicates, **sorts** (so the query
//! is independent of word order — the determinism the Phase 21 gate requires),
//! caps the term count, and joins the quoted terms with ` OR `.

/// Max terms in a generated query.
pub const MAX_QUERY_TERMS: usize = 24;
/// Tokens shorter than this are dropped.
const MIN_TERM_CHARS: usize = 3;

/// Common words that carry no retrieval signal. Deliberately tiny — FTS5 BM25
/// already down-weights frequent terms; this just trims obvious noise.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "are", "but", "not", "you", "your", "yours", "was", "were", "his", "her",
    "hers", "she", "him", "our", "ours", "their", "them", "they", "this", "that", "these", "those",
    "with", "from", "have", "has", "had", "will", "would", "should", "could", "can", "did", "does",
    "what", "when", "where", "who", "why", "how", "there", "here", "then", "than", "into", "out",
    "about", "just", "like", "get", "got", "let", "yeah", "okay", "sure",
];

/// Build an FTS5 MATCH query from `turn`, or `None` when nothing usable remains.
#[must_use]
pub fn fts_query_for(turn: &str) -> Option<String> {
    let mut terms: Vec<String> = turn
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= MIN_TERM_CHARS)
        .filter(|t| !STOPWORDS.contains(t))
        .map(str::to_owned)
        .collect();
    terms.sort_unstable();
    terms.dedup();
    if terms.is_empty() {
        return None;
    }
    terms.truncate(MAX_QUERY_TERMS);
    Some(
        terms
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(" OR "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_of_word_order() {
        let a = fts_query_for("the cat sat on the warm mat").unwrap();
        let b = fts_query_for("mat warm the on sat cat the").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn drops_stopwords_short_tokens_and_punctuation() {
        let q = fts_query_for("I have a dog!! and a HUGE cat.").unwrap();
        assert!(q.contains("\"dog\""));
        assert!(q.contains("\"huge\""));
        assert!(q.contains("\"cat\""));
        assert!(!q.contains("have"));
        assert!(!q.contains("\"a\""));
        assert!(!q.contains('!'));
    }

    #[test]
    fn nothing_usable_is_none() {
        assert_eq!(fts_query_for("  ok, and the ... "), None);
        assert_eq!(fts_query_for(""), None);
    }

    #[test]
    fn caps_the_term_count() {
        let many = (0..100)
            .map(|i| format!("word{i:03}"))
            .collect::<Vec<_>>()
            .join(" ");
        let q = fts_query_for(&many).unwrap();
        assert_eq!(q.matches(" OR ").count(), MAX_QUERY_TERMS - 1);
    }
}
