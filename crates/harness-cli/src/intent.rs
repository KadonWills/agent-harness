//! Traceability: link a session back to the instruction that produced it.

use sha2::{Digest, Sha256};

/// Trim and collapse whitespace so cosmetic reformatting of a prompt does
/// not change its hash.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// SHA-256 hex of the normalized intent, or `""` when there is none.
pub fn hash(text: Option<&str>) -> String {
    let normalized = normalize(text.unwrap_or_default());
    if normalized.is_empty() {
        return String::new();
    }
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A session id unique per run on this machine.
pub fn session_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "harness_session_{:x}{:08x}_{:x}",
        now.as_secs(),
        now.subsec_nanos(),
        std::process::id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ignores_cosmetic_whitespace() {
        let a = hash(Some("Add a  login page\n"));
        let b = hash(Some("  Add a login\tpage"));
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
        assert_ne!(a, hash(Some("Add a logout page")));
    }

    #[test]
    fn no_intent_means_empty_hash() {
        assert_eq!(hash(None), "");
        assert_eq!(hash(Some("  \n")), "");
    }

    #[test]
    fn known_vector() {
        assert_eq!(
            hash(Some("abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn session_ids_are_prefixed() {
        assert!(session_id().starts_with("harness_session_"));
    }
}
