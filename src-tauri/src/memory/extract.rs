//! Post-turn memory extraction: a schema-constrained LLM pass over one
//! conversation exchange → candidate memories → the validation gate.
//!
//! The LLM output is **untrusted** — parsed against a strict JSON shape, then
//! importance / length / dedup gated, then `sanitize::strip_control`led before
//! it reaches the repo. A malformed or empty pass stores nothing and logs; it
//! is never fatal (it runs off the response path).

#![allow(clippy::cast_precision_loss)]

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{
    MemoryRepo, NewMemory, DEDUP_JACCARD, MAX_CONTENT_CHARS, MIN_CONTENT_CHARS, MIN_IMPORTANCE,
};
use crate::context::sanitize::strip_control;
use crate::contracts::generation::SamplingParams;
use crate::contracts::ids::ConversationId;
use crate::contracts::memory::MemoryKind;
use crate::lifecycle::backend::LlmInstance;

/// Hard cap on the extraction generation.
const EXTRACT_MAX_TOKENS: u32 = 256;

/// One conversation exchange to mine for memories.
pub struct Exchange {
    /// The conversation it belongs to (provenance).
    pub conversation_id: ConversationId,
    /// The assistant message id (provenance).
    pub assistant_message_id: Option<String>,
    /// What the user said.
    pub user_text: String,
    /// What the assistant replied.
    pub assistant_text: String,
}

/// A memory the LLM proposed, before validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryCandidate {
    /// The proposed text.
    pub content: String,
    /// Category.
    pub kind: MemoryKind,
    /// 1..=5.
    pub importance: u32,
}

const SYSTEM: &str = "You extract durable, long-term facts about the user that are worth \
remembering in future conversations. Output ONLY a JSON object of the form \
{\"memories\":[{\"content\":\"...\",\"kind\":\"Fact|Preference|Event|Trait\",\"importance\":1-5}]}. \
Use importance 1 for trivia and 5 for defining facts. If nothing is worth remembering \
long-term, output {\"memories\":[]}. Never remember greetings, questions, the assistant's \
own words, or transient state.";

/// Build the extraction prompt for an exchange (ChatML).
fn prompt_for(ex: &Exchange) -> String {
    format!(
        "<|im_start|>system\n{SYSTEM}<|im_end|>\n\
         <|im_start|>user\nThe user said:\n{user}\n\nThe assistant replied:\n{assistant}\
         \n\nExtract the durable memories.<|im_end|>\n<|im_start|>assistant\n",
        user = strip_control(&ex.user_text),
        assistant = strip_control(&ex.assistant_text),
    )
}

fn sampling() -> SamplingParams {
    SamplingParams {
        temperature: Some(0.0),
        top_p: None,
        top_k: None,
        max_tokens: Some(EXTRACT_MAX_TOKENS),
        stop: vec!["<|im_end|>".to_owned()],
        seed: Some(0),
    }
}

#[derive(Deserialize)]
struct ExtractionOutput {
    #[serde(default)]
    memories: Vec<RawCandidate>,
}

#[derive(Deserialize)]
struct RawCandidate {
    #[serde(default)]
    content: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    importance: u32,
}

/// Run one extraction pass. Returns the raw candidates (unvalidated). An
/// unparseable response yields an empty vec + a `warn`.
pub async fn run_extraction(
    llm: &dyn LlmInstance,
    exchange: &Exchange,
    cancel: CancellationToken,
) -> Vec<MemoryCandidate> {
    if cancel.is_cancelled() {
        return Vec::new();
    }
    let completion = match llm.generate(prompt_for(exchange), sampling(), cancel).await {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(target: "memory", %err, "extraction generation failed");
            return Vec::new();
        }
    };
    parse_candidates(&completion.text)
}

/// Extract the first `{ … }` span and parse it as the extraction schema.
fn parse_candidates(text: &str) -> Vec<MemoryCandidate> {
    let Some(json) = json_object_span(text) else {
        tracing::warn!(target: "memory", "extraction response had no JSON object");
        return Vec::new();
    };
    let parsed: ExtractionOutput = match serde_json::from_str(json) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!(target: "memory", %err, "extraction JSON did not match the schema");
            return Vec::new();
        }
    };
    parsed
        .memories
        .into_iter()
        .filter_map(|r| {
            Some(MemoryCandidate {
                content: r.content.trim().to_owned(),
                kind: kind_from_str(&r.kind)?,
                importance: r.importance.clamp(1, 5),
            })
        })
        .collect()
}

fn json_object_span(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

/// Jaccard similarity of the two texts' lower-cased word sets (tokens ≥ 3
/// chars). `1.0` = identical vocabulary, `0.0` = disjoint.
fn word_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let words = |s: &str| -> HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.chars().count() >= 3)
            .map(str::to_owned)
            .collect()
    };
    let (sa, sb) = (words(a), words(b));
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    inter / union
}

fn kind_from_str(s: &str) -> Option<MemoryKind> {
    match s.trim().to_ascii_lowercase().as_str() {
        "fact" => Some(MemoryKind::Fact),
        "preference" => Some(MemoryKind::Preference),
        "event" => Some(MemoryKind::Event),
        "trait" => Some(MemoryKind::Trait),
        _ => None,
    }
}

/// The validation gate — importance, length, and word-overlap dedup against
/// every existing memory in `scope`. Returns the row to store, or `None`
/// (with the reason logged at debug).
pub async fn validate(
    candidate: &MemoryCandidate,
    repo: &MemoryRepo,
    scope: &str,
    exchange: &Exchange,
) -> Option<NewMemory> {
    if candidate.importance < MIN_IMPORTANCE {
        tracing::debug!(target: "memory", importance = candidate.importance, "candidate below the importance floor");
        return None;
    }
    let content = strip_control(&candidate.content);
    let chars = content.chars().count();
    if !(MIN_CONTENT_CHARS..=MAX_CONTENT_CHARS).contains(&chars) {
        tracing::debug!(target: "memory", chars, "candidate content length out of range");
        return None;
    }
    // Dedup scans the *whole* scope (bounded by PER_SCOPE_CAP=500, cheap),
    // not just an FTS keyword top-K — a keyword search can rank a genuine
    // near-duplicate outside a small K when other rows share more common
    // words, letting reworded repeats of the same fact pile up. This does
    // not catch paraphrases with near-zero word overlap ("loves cats" vs.
    // "is a big fan of felines") — that needs semantic (embedding) dedup,
    // deferred per ADR-0012.
    match repo.list(scope).await {
        Ok(existing) => {
            if let Some(sim) = existing
                .iter()
                .map(|m| word_jaccard(&content, &m.content))
                .reduce(f64::max)
            {
                if sim >= DEDUP_JACCARD {
                    tracing::debug!(target: "memory", sim, "candidate is a near-duplicate of an existing memory");
                    return None;
                }
            }
        }
        Err(err) => {
            // Fail open but *visibly* — better a rare stored duplicate than a
            // silently swallowed error (the previous behaviour).
            tracing::warn!(target: "memory", %err, "dedup scope scan failed — storing candidate unchecked");
        }
    }
    Some(NewMemory {
        scope: scope.to_owned(),
        kind: candidate.kind,
        content,
        importance: candidate.importance,
        source_conversation_id: Some(exchange.conversation_id.clone()),
        source_message_id: exchange.assistant_message_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_block_with_surrounding_noise() {
        let text = "Sure! Here you go:\n{\"memories\":[\
            {\"content\":\"plays the cello\",\"kind\":\"Trait\",\"importance\":4},\
            {\"content\":\"moved to Berlin\",\"kind\":\"event\",\"importance\":3}]}\nHope that helps.";
        let got = parse_candidates(text);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kind, MemoryKind::Trait);
        assert_eq!(got[1].kind, MemoryKind::Event); // case-insensitive
    }

    #[test]
    fn malformed_or_empty_yields_nothing() {
        assert!(parse_candidates("no json here").is_empty());
        assert!(parse_candidates("{not valid json}").is_empty());
        assert!(parse_candidates("{\"memories\":[]}").is_empty());
        assert!(parse_candidates("{\"other\":1}").is_empty());
    }

    #[test]
    fn unknown_kind_and_out_of_range_importance_are_handled() {
        let got = parse_candidates(
            "{\"memories\":[\
             {\"content\":\"x\",\"kind\":\"Grudge\",\"importance\":3},\
             {\"content\":\"y\",\"kind\":\"Fact\",\"importance\":99}]}",
        );
        assert_eq!(got.len(), 1); // the Grudge candidate is dropped
        assert_eq!(got[0].importance, 5); // clamped
    }
}
