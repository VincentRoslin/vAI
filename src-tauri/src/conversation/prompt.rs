//! Minimal ChatML prompt rendering for the Phase 16 slice.
//!
//! This is deliberately tiny — the real context builder (persona + memory +
//! character + runtime) is Phase 20. Here: a system line, the transcript, and
//! the assistant tail, in the ChatML format Qwen/Llama-3-style instruct models
//! expect.

use crate::contracts::conversation::{Message, MessageContent, Role};

/// The default system instruction for a bare Persona chat.
pub const DEFAULT_SYSTEM: &str = "You are a helpful assistant.";

/// Render `messages` (in order) into a ChatML prompt ending with the open
/// assistant turn. A turn contributes text when it is [`MessageContent::Text`]
/// or a transcribed [`MessageContent::Audio`] (voice, Phase 18); an image turn
/// or an untranscribed audio turn is skipped.
#[must_use]
pub fn render_chatml(messages: &[Message], system: &str) -> String {
    let mut out = String::new();
    push_turn(&mut out, "system", system);
    for msg in messages {
        if let Some(text) = turn_text(&msg.content) {
            push_turn(&mut out, role_tag(msg.role), text);
        }
    }
    out.push_str("<|im_start|>assistant\n");
    out
}

/// The text a turn contributes to the prompt, if any.
fn turn_text(content: &MessageContent) -> Option<&str> {
    match content {
        MessageContent::Text { text } => Some(text),
        MessageContent::Audio {
            transcript: Some(t),
            ..
        } => Some(t),
        MessageContent::Audio {
            transcript: None, ..
        }
        | MessageContent::Image { .. } => None,
    }
}

fn push_turn(out: &mut String, role: &str, content: &str) {
    out.push_str("<|im_start|>");
    out.push_str(role);
    out.push('\n');
    out.push_str(content);
    out.push_str("<|im_end|>\n");
}

fn role_tag(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}
