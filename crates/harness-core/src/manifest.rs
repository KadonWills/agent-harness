//! The `AgentManifest.json` contract emitted after every harness session.
//!
//! This is AgentHarness's public data interface: CI systems, dashboards,
//! and other tools consume it. Changes here are semver-relevant and
//! require an RFC (see docs/rfcs/). Fields added by RFC-0001 deserialize
//! with defaults, so manifests written by earlier builds still parse.

use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub manifest_version: String,
    pub session_id: String,
    pub target_agent: String,
    /// SHA-256 of the normalized agent instruction/prompt, for traceability.
    /// Empty when no intent was supplied.
    pub intent_hash: String,
    /// The gate decision.
    #[serde(default)]
    pub verdict: Verdict,
    /// One agent-legible sentence per reason the gate blocked.
    #[serde(default)]
    pub blocking_reasons: Vec<String>,
    /// The ref the change was diffed against.
    #[serde(default)]
    pub base_ref: String,
    /// The HEAD commit SHA at the time of the run.
    #[serde(default)]
    pub head: String,
    pub impacted_files: Vec<ImpactedFile>,
    pub sandbox_status: SandboxStatus,
    /// Every sandbox step that ran, in order.
    #[serde(default)]
    pub steps: Vec<StepReport>,
    pub security_audit: SecurityAudit,
    /// Semantic conflicts with sibling agent branches.
    #[serde(default)]
    pub conflicts: Vec<ConflictReport>,
}

impl AgentManifest {
    /// A manifest with every list empty and the verdict still undecided.
    pub fn new(session_id: impl Into<String>, target_agent: impl Into<String>) -> Self {
        Self {
            manifest_version: MANIFEST_VERSION.into(),
            session_id: session_id.into(),
            target_agent: target_agent.into(),
            intent_hash: String::new(),
            verdict: Verdict::Unknown,
            blocking_reasons: Vec::new(),
            base_ref: String::new(),
            head: String::new(),
            impacted_files: Vec::new(),
            sandbox_status: SandboxStatus::Skipped,
            steps: Vec::new(),
            security_audit: SecurityAudit::default(),
            conflicts: Vec::new(),
        }
    }

    /// Record a reason to block. The verdict is derived in [`Self::finalize`].
    pub fn block(&mut self, reason: impl Into<String>) {
        self.blocking_reasons.push(reason.into());
    }

    /// Sync the audit counters with their lists and decide the verdict.
    pub fn finalize(&mut self) {
        let audit = &mut self.security_audit;
        audit.unauthorized_egress_attempts = count(audit.egress_events.len());
        audit.secrets_blocked = count(audit.secret_findings.len());
        self.verdict = if self.blocking_reasons.is_empty() {
            Verdict::Pass
        } else {
            Verdict::Blocked
        };
    }

    /// Serialize as pretty JSON (the on-disk format).
    pub fn to_json(&self) -> Result<String, crate::Error> {
        serde_json::to_string_pretty(self).map_err(|e| crate::Error::Manifest(e.to_string()))
    }

    /// Parse a manifest, including ones written before RFC-0001.
    pub fn from_json(input: &str) -> Result<Self, crate::Error> {
        serde_json::from_str(input).map_err(|e| crate::Error::Manifest(e.to_string()))
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    Pass,
    Blocked,
    /// Only seen when reading a manifest written before verdicts existed.
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactedFile {
    pub path: String,
    /// `<Variant>:<symbol>` entries, e.g. `FunctionChanged:resolveUser`.
    pub ast_changes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Setup,
    Validate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepReport {
    pub phase: Phase,
    pub command: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityAudit {
    pub unauthorized_egress_attempts: u32,
    pub secrets_blocked: u32,
    #[serde(default)]
    pub egress_events: Vec<EgressEvent>,
    #[serde(default)]
    pub secret_findings: Vec<SecretFinding>,
    #[serde(default)]
    pub dependency_findings: Vec<DependencyFinding>,
}

/// A denied network attempt observed inside the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EgressEvent {
    /// The command that made the attempt.
    pub step: String,
    /// Host or address, or `"unknown"` when the evidence names none.
    pub destination: String,
    /// The output line the attempt was detected from.
    pub evidence: String,
}

/// A secret found in added lines. The secret itself is never recorded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SecretFinding {
    pub rule_id: String,
    pub description: String,
    pub path: String,
    pub line: u32,
}

/// A newly introduced dependency that violates policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DependencyFinding {
    /// "npm", "pypi", "cargo", or "go".
    pub ecosystem: String,
    pub name: String,
    pub file: String,
    pub reason: String,
}

/// A semantic conflict between two change sets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConflictReport {
    /// 1 = auto-fixable by a deterministic codemod, 2 = halt and escalate.
    pub level: u8,
    /// Rule name, e.g. "removed-symbol".
    pub kind: String,
    pub symbol: String,
    /// `[a, b]`: `a` made the change that `b` collides with.
    pub branches: [String; 2],
    pub explanation: String,
    pub fix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_through_json() {
        let mut m = AgentManifest::new("harness_session_test", "claude-code");
        m.steps.push(StepReport {
            phase: Phase::Validate,
            command: "npm test".into(),
            exit_code: 0,
            duration_ms: 12,
            timed_out: false,
        });
        m.finalize();
        let json = m.to_json().expect("serialize");
        assert!(json.contains("\"phase\": \"validate\""), "{json}");
        let back = AgentManifest::from_json(&json).expect("deserialize");
        assert_eq!(back.verdict, Verdict::Pass);
        assert_eq!(back.steps, m.steps);
    }

    #[test]
    fn pre_rfc_manifests_still_parse() {
        let old = r#"{
            "manifest_version": "1",
            "session_id": "s",
            "target_agent": "cline",
            "intent_hash": "e3b0c442",
            "impacted_files": [{"path": "a.ts", "ast_changes": []}],
            "sandbox_status": "Passed",
            "security_audit": {"unauthorized_egress_attempts": 0, "secrets_blocked": 0}
        }"#;
        let m = AgentManifest::from_json(old).expect("old manifest must parse");
        assert_eq!(m.verdict, Verdict::Unknown);
        assert!(m.conflicts.is_empty());
        assert!(m.security_audit.secret_findings.is_empty());
    }

    #[test]
    fn finalize_derives_verdict_and_counters() {
        let mut m = AgentManifest::new("s", "a");
        m.security_audit.egress_events.push(EgressEvent {
            step: "npm test".into(),
            destination: "registry.npmjs.org".into(),
            evidence: "getaddrinfo ENOTFOUND registry.npmjs.org".into(),
        });
        m.block("1 denied network attempt during `npm test`");
        m.finalize();
        assert_eq!(m.verdict, Verdict::Blocked);
        assert_eq!(m.security_audit.unauthorized_egress_attempts, 1);
        assert_eq!(m.security_audit.secrets_blocked, 0);
    }

    #[test]
    fn invalid_json_is_a_manifest_error() {
        assert!(matches!(
            AgentManifest::from_json("{"),
            Err(crate::Error::Manifest(_))
        ));
    }
}
