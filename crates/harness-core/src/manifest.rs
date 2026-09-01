//! The `AgentManifest.json` contract emitted after every harness session.
//!
//! This is AgentHarness's public data interface: CI systems, dashboards,
//! and other tools consume it. Changes here are semver-relevant and
//! require an RFC (see docs/rfcs/).

use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: &str = "1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub manifest_version: String,
    pub session_id: String,
    pub target_agent: String,
    /// SHA-256 of the normalized agent instruction/prompt, for traceability.
    pub intent_hash: String,
    pub impacted_files: Vec<ImpactedFile>,
    pub sandbox_status: SandboxStatus,
    pub security_audit: SecurityAudit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactedFile {
    pub path: String,
    pub ast_changes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SecurityAudit {
    pub unauthorized_egress_attempts: u32,
    pub secrets_blocked: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_through_json() {
        let m = AgentManifest {
            manifest_version: MANIFEST_VERSION.into(),
            session_id: "harness_session_test".into(),
            target_agent: "claude-code".into(),
            intent_hash: "e3b0c442".into(),
            impacted_files: vec![],
            sandbox_status: SandboxStatus::Passed,
            security_audit: SecurityAudit::default(),
        };
        let json = serde_json::to_string(&m).expect("serialize");
        let back: AgentManifest = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.sandbox_status, SandboxStatus::Passed);
    }
}
