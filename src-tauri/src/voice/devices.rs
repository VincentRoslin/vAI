//! Shared WASAPI device matching.
//!
//! Gaming headsets typically show up as several endpoints ("Chat" vs "Game" vs
//! 7.1). Names also drift in case or gain a USB-port suffix. We match exact
//! first, then case-insensitive / substring, then the host default.

/// True when `want` (config) refers to `have` (a live `cpal` name).
#[must_use]
pub fn names_match(want: &str, have: &str) -> bool {
    let a = want.trim();
    let b = have.trim();
    if a == b {
        return true;
    }
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    a == b || (!a.is_empty() && (a.contains(&b) || b.contains(&a)))
}

/// Sort key for voice-call endpoints. Lower is better. Chat/Headset beat
/// Game/7.1/Speakers so a gaming headset's communications path is easy to pick.
#[must_use]
pub fn communications_rank(name: &str) -> u8 {
    let n = name.to_lowercase();
    if n.contains("chat") || n.contains("communications") {
        0
    } else if n.contains("game") || n.contains("7.1") || n.contains("surround") {
        3
    } else if n.contains("headset") || n.contains("headphone") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_case_and_substring() {
        assert!(names_match("Headset Microphone (HyperX)", "Headset Microphone (HyperX)"));
        assert!(names_match(
            "headset microphone (hyperx)",
            "Headset Microphone (HyperX)"
        ));
        assert!(names_match("HyperX", "Headset Microphone (HyperX Cloud II)"));
        assert!(!names_match("USB Mic", "Speakers (Realtek)"));
        assert!(!names_match("", "Speakers"));
    }

    #[test]
    fn chat_ranks_ahead_of_game() {
        assert!(
            communications_rank("Headset Earphone (HyperX Chat)")
                < communications_rank("Speakers (Realtek)")
        );
        assert!(
            communications_rank("Headset Microphone (HyperX Chat)")
                < communications_rank("Headphones (HyperX Cloud II Game)")
        );
        assert_eq!(communications_rank("USB PnP Sound Device"), 2);
    }
}
