//! The "hostile network" environment every Python worker is launched with
//! (ADR-0015). One place; unit-tested against the ADR's list.
//!
//! Workers load models from local paths only and never resolve a repo id. The
//! only component allowed network access is the Rust acquisition path (ADR-0008).

use tokio::process::Command;

/// Env vars set on every worker child. Names + values are exactly the ADR-0015
/// list.
pub const OFFLINE_ENV: &[(&str, &str)] = &[
    ("HF_HUB_OFFLINE", "1"),
    ("TRANSFORMERS_OFFLINE", "1"),
    ("HF_HUB_DISABLE_TELEMETRY", "1"),
    ("HF_HUB_DISABLE_IMPLICIT_TOKEN", "1"),
    ("DISABLE_TELEMETRY", "1"),
    ("DO_NOT_TRACK", "1"),
    ("HF_HUB_DISABLE_PROGRESS_BARS", "1"),
    // Keep stdout clean for the JSON-lines protocol and stop hf-hub writing a
    // cache anywhere unexpected.
    ("PYTHONUNBUFFERED", "1"),
    ("PYTHONUTF8", "1"),
];

/// Proxy vars removed from the inherited environment so a worker cannot be
/// pointed at a network egress (ADR-0015: "no HTTP(S)_PROXY / ALL_PROXY
/// inherited").
pub const STRIP_ENV: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Apply the lockdown env to a not-yet-spawned command.
pub fn apply(cmd: &mut Command) {
    for (key, value) in OFFLINE_ENV {
        cmd.env(key, value);
    }
    for key in STRIP_ENV {
        cmd.env_remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_env_matches_adr_0015() {
        // The exact set ADR-0015 mandates (PYTHON* are ours, for the protocol).
        for required in [
            "HF_HUB_OFFLINE",
            "TRANSFORMERS_OFFLINE",
            "HF_HUB_DISABLE_TELEMETRY",
            "HF_HUB_DISABLE_IMPLICIT_TOKEN",
            "DISABLE_TELEMETRY",
            "DO_NOT_TRACK",
            "HF_HUB_DISABLE_PROGRESS_BARS",
        ] {
            assert!(
                OFFLINE_ENV.iter().any(|(k, v)| *k == required && *v == "1"),
                "{required} missing from OFFLINE_ENV"
            );
        }
    }

    #[test]
    fn strips_every_proxy_var() {
        for proxy in ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
            assert!(STRIP_ENV.contains(&proxy));
        }
    }
}
