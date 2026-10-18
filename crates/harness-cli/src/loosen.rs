//! Config edits that weaken the gate (RFC-0002 §7). The UIs warn about
//! them, and the web UI refuses to save them without explicit confirmation.

use harness_core::config::HarnessConfig;

/// One sentence per setting that `after` loosens relative to `before`.
pub fn loosened(before: &HarnessConfig, after: &HarnessConfig) -> Vec<String> {
    let mut out = Vec::new();
    let mut enabled = |field: &str, was: bool, now: bool, effect: &str| {
        if !was && now {
            out.push(format!("{field} turned on: {effect}"));
        }
    };
    enabled(
        "sandbox.allow_egress",
        before.sandbox.allow_egress,
        after.sandbox.allow_egress,
        "validation commands can reach the network",
    );
    enabled(
        "sandbox.allow_setup_egress",
        before.sandbox.allow_setup_egress,
        after.sandbox.allow_setup_egress,
        "setup commands can reach the network",
    );
    let mut disabled = |field: &str, was: bool, now: bool, effect: &str| {
        if was && !now {
            out.push(format!("{field} turned off: {effect}"));
        }
    };
    disabled(
        "security.scan_secrets",
        before.security.scan_secrets,
        after.security.scan_secrets,
        "added secrets are no longer blocked",
    );
    disabled(
        "security.dependencies.typosquat_check",
        before.security.dependencies.typosquat_check,
        after.security.dependencies.typosquat_check,
        "look-alike package names are no longer blocked",
    );
    disabled(
        "agents.strict_mode",
        before.agents.strict_mode,
        after.agents.strict_mode,
        "unlisted agents are no longer blocked",
    );
    if !before.agents.allowed.is_empty() && after.agents.allowed.is_empty() {
        out.push("agents.allowed emptied: any agent id is accepted".to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(toml: &str) -> HarnessConfig {
        HarnessConfig::from_toml(toml).expect("parses")
    }

    #[test]
    fn tightening_and_unrelated_edits_are_not_loosening() {
        let base = cfg("[agents]\nallowed = [\"a\"]\n");
        let tighter = cfg("[agents]\nallowed = [\"a\"]\n[sandbox]\ntimeout = \"1m\"\n");
        assert!(loosened(&base, &tighter).is_empty());
        assert!(loosened(&cfg("[sandbox]\nallow_egress = true\n"), &base).is_empty());
    }

    #[test]
    fn every_loosening_is_named() {
        let before = cfg("[agents]\nallowed = [\"a\"]\nstrict_mode = true\n");
        let after = cfg(
            "[sandbox]\nallow_egress = true\nallow_setup_egress = true\n\
             [agents]\nstrict_mode = false\n\
             [security]\nscan_secrets = false\n\
             [security.dependencies]\ntyposquat_check = false\n",
        );
        let found = loosened(&before, &after);
        let fields: Vec<&str> = found
            .iter()
            .map(|s| s.split(' ').next().unwrap_or_default())
            .collect();
        assert_eq!(
            fields,
            [
                "sandbox.allow_egress",
                "sandbox.allow_setup_egress",
                "security.scan_secrets",
                "security.dependencies.typosquat_check",
                "agents.strict_mode",
                "agents.allowed",
            ]
        );
    }
}
