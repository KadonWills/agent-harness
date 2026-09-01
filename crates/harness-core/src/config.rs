//! The `harness.toml` configuration schema.
//!
//! Design goals (see docs/PRD.md §4.1):
//! - Declarative and diff-friendly, so agents can generate and self-correct it.
//! - Every field has a safe default; an empty file is a valid config.

use serde::{Deserialize, Serialize};

/// Root of a `harness.toml` file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    /// Sandbox backend: "docker" (default) or "wasmtime".
    #[serde(default = "default_provider")]
    pub provider: String,
    /// Outbound network from the sandbox. Defaults to false (zero-trust).
    #[serde(default)]
    pub allow_egress: bool,
    /// Memory ceiling, e.g. "512MB".
    #[serde(default = "default_max_memory")]
    pub max_memory: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            allow_egress: false,
            max_memory: default_max_memory(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentsConfig {
    /// Agent identifiers permitted to open harness sessions. Empty = allow all.
    #[serde(default)]
    pub allowed: Vec<String>,
    /// When true, sessions from unlisted agents are rejected instead of warned.
    #[serde(default)]
    pub strict_mode: bool,
}

fn default_provider() -> String {
    "docker".to_string()
}

fn default_max_memory() -> String {
    "512MB".to_string()
}

impl HarnessConfig {
    /// Parse a `harness.toml` document.
    pub fn from_toml(input: &str) -> Result<Self, crate::Error> {
        toml::from_str(input).map_err(|e| crate::Error::Config(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_valid_with_safe_defaults() {
        let cfg = HarnessConfig::from_toml("").expect("empty config must parse");
        assert!(!cfg.sandbox.allow_egress, "egress must default to off");
        assert_eq!(cfg.sandbox.provider, "docker");
    }

    #[test]
    fn example_config_parses() {
        let cfg = HarnessConfig::from_toml(include_str!("../../../examples/harness.toml"))
            .expect("examples/harness.toml must stay valid");
        assert_eq!(cfg.project.name, "core-api");
        assert!(cfg.agents.strict_mode);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(HarnessConfig::from_toml("[sandbox]\ntypo_field = true\n").is_err());
    }
}
