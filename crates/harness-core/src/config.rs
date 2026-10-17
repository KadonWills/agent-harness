//! The `harness.toml` configuration schema.
//!
//! Design goals (see docs/PRD.md §4.1 and docs/rfcs/0001-v0.1-v0.4-contracts.md):
//! - Declarative and diff-friendly, so agents can generate and self-correct it.
//! - Every field has a safe default; an empty file is a valid config.

use std::time::Duration;

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
    pub validation: ValidationConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub conflicts: ConflictsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default)]
    pub name: String,
    /// Ref that `harness run` diffs against.
    #[serde(default = "default_base_ref")]
    pub base_ref: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            base_ref: default_base_ref(),
        }
    }
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
    /// Container image for the Docker backend.
    #[serde(default = "default_image")]
    pub image: String,
    /// Wall-clock budget for the whole session, e.g. "10m".
    #[serde(default = "default_timeout")]
    pub timeout: String,
    /// Per-process CPU-time ceiling, e.g. "5m".
    #[serde(default = "default_max_cpu_time")]
    pub max_cpu_time: String,
    /// Network access for `[validation].setup` only. Defaults to false.
    #[serde(default)]
    pub allow_setup_egress: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            allow_egress: false,
            max_memory: default_max_memory(),
            image: default_image(),
            timeout: default_timeout(),
            max_cpu_time: default_max_cpu_time(),
            allow_setup_egress: false,
        }
    }
}

/// The project's own validation commands, run inside the sandbox.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationConfig {
    /// Run first (dependency install). May get network via
    /// `sandbox.allow_setup_egress`.
    #[serde(default)]
    pub setup: Vec<String>,
    /// Run after setup; network per `sandbox.allow_egress`.
    #[serde(default)]
    pub commands: Vec<String>,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictsConfig {
    /// Sibling-branch globs (e.g. "agent/*") compared on every run.
    #[serde(default)]
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    /// Scan added lines for secrets. Defaults to true.
    #[serde(default = "default_true")]
    pub scan_secrets: bool,
    /// Optional gitleaks-format rules file, merged with the built-in rules.
    #[serde(default)]
    pub secret_rules: String,
    #[serde(default)]
    pub dependencies: DependencyPolicyConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            scan_secrets: true,
            secret_rules: String::new(),
            dependencies: DependencyPolicyConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyPolicyConfig {
    /// Globs; when non-empty, every new dependency must match one.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Globs; matching new dependencies are blocked.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Block new dependencies one edit away from a popular package name.
    #[serde(default = "default_true")]
    pub typosquat_check: bool,
    /// Optional directory of OSV JSON advisories (offline snapshot).
    #[serde(default)]
    pub advisory_db: String,
}

impl Default for DependencyPolicyConfig {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            deny: Vec::new(),
            typosquat_check: true,
            advisory_db: String::new(),
        }
    }
}

fn default_base_ref() -> String {
    "main".to_string()
}

fn default_provider() -> String {
    "docker".to_string()
}

fn default_max_memory() -> String {
    "512MB".to_string()
}

fn default_image() -> String {
    "debian:bookworm-slim".to_string()
}

fn default_timeout() -> String {
    "10m".to_string()
}

fn default_max_cpu_time() -> String {
    "5m".to_string()
}

fn default_true() -> bool {
    true
}

/// Sandbox providers this build understands.
pub const PROVIDERS: &[&str] = &["docker", "wasmtime"];

/// One problem found by [`HarnessConfig::validate`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// Dotted path of the offending field, e.g. "sandbox.timeout".
    pub field: String,
    /// What is wrong.
    pub message: String,
    /// What to do about it.
    pub fix: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} — fix: {}", self.field, self.message, self.fix)
    }
}

impl HarnessConfig {
    /// Parse a `harness.toml` document.
    pub fn from_toml(input: &str) -> Result<Self, crate::Error> {
        toml::from_str(input).map_err(|e| {
            crate::Error::Config(format!(
                "{e}\nfix: correct the field above; the full schema is in \
                 examples/harness.toml (unknown fields are always rejected)"
            ))
        })
    }

    /// Semantic checks beyond parsing. An empty result means the config is
    /// usable as-is.
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        let mut push = |field: &str, message: String, fix: &str| {
            problems.push(Problem {
                field: field.to_string(),
                message,
                fix: fix.to_string(),
            });
        };

        if !PROVIDERS.contains(&self.sandbox.provider.as_str()) {
            push(
                "sandbox.provider",
                format!("unknown provider {:?}", self.sandbox.provider),
                "use \"docker\" or \"wasmtime\"",
            );
        }
        if let Err(e) = parse_memory(&self.sandbox.max_memory) {
            push(
                "sandbox.max_memory",
                e,
                "use a size like \"512MB\" or \"2GB\"",
            );
        }
        for (field, value) in [
            ("sandbox.timeout", &self.sandbox.timeout),
            ("sandbox.max_cpu_time", &self.sandbox.max_cpu_time),
        ] {
            if let Err(e) = parse_duration(value) {
                push(field, e, "use a duration like \"30s\", \"10m\" or \"1h\"");
            }
        }
        if self.sandbox.provider == "docker" && self.sandbox.image.trim().is_empty() {
            push(
                "sandbox.image",
                "the docker provider needs an image".to_string(),
                "set an image, e.g. image = \"node:22-bookworm-slim\"",
            );
        }
        if self.project.base_ref.trim().is_empty() {
            push(
                "project.base_ref",
                "base_ref is empty".to_string(),
                "set the branch agents start from, e.g. base_ref = \"main\"",
            );
        }
        for (i, cmd) in self
            .validation
            .setup
            .iter()
            .chain(&self.validation.commands)
            .enumerate()
        {
            if cmd.trim().is_empty() {
                push(
                    "validation",
                    format!("command #{} is empty", i + 1),
                    "remove the empty string from setup/commands",
                );
            }
        }
        if self.agents.strict_mode && self.agents.allowed.is_empty() {
            push(
                "agents.strict_mode",
                "strict_mode is on but agents.allowed is empty, so every session is rejected"
                    .to_string(),
                "list permitted agents in agents.allowed, or set strict_mode = false",
            );
        }
        problems
    }
}

/// Parse a duration such as `"30s"`, `"10m"`, or `"1h"`.
pub fn parse_duration(input: &str) -> Result<Duration, String> {
    let s = input.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num
        .parse()
        .map_err(|_| format!("{input:?} is not a duration"))?;
    let secs = match unit.trim() {
        "s" => n,
        "m" => n.saturating_mul(60),
        "h" => n.saturating_mul(3600),
        _ => return Err(format!("{input:?} has no unit (expected s, m, or h)")),
    };
    if secs == 0 {
        return Err(format!("{input:?} must be greater than zero"));
    }
    Ok(Duration::from_secs(secs))
}

/// Parse a memory size such as `"512MB"` into bytes (binary multiples).
pub fn parse_memory(input: &str) -> Result<u64, String> {
    let s = input.trim();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: u64 = num
        .parse()
        .map_err(|_| format!("{input:?} is not a memory size"))?;
    let mult: u64 = match unit.trim().to_ascii_uppercase().as_str() {
        "KB" => 1 << 10,
        "MB" => 1 << 20,
        "GB" => 1 << 30,
        _ => return Err(format!("{input:?} has no unit (expected KB, MB, or GB)")),
    };
    if n == 0 {
        return Err(format!("{input:?} must be greater than zero"));
    }
    Ok(n.saturating_mul(mult))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_valid_with_safe_defaults() {
        let cfg = HarnessConfig::from_toml("").expect("empty config must parse");
        assert!(!cfg.sandbox.allow_egress, "egress must default to off");
        assert!(
            !cfg.sandbox.allow_setup_egress,
            "setup egress must default to off"
        );
        assert_eq!(cfg.sandbox.provider, "docker");
        assert_eq!(cfg.project.base_ref, "main");
        assert!(cfg.security.scan_secrets);
        assert!(cfg.security.dependencies.typosquat_check);
        assert!(cfg.validate().is_empty(), "{:?}", cfg.validate());
    }

    #[test]
    fn example_config_parses() {
        let cfg = HarnessConfig::from_toml(include_str!("../../../examples/harness.toml"))
            .expect("examples/harness.toml must stay valid");
        assert_eq!(cfg.project.name, "core-api");
        assert!(cfg.agents.strict_mode);
        assert!(cfg.validate().is_empty(), "{:?}", cfg.validate());
    }

    #[test]
    fn unknown_fields_are_rejected_with_a_fix() {
        let err =
            HarnessConfig::from_toml("[sandbox]\ntypo_field = true\n").expect_err("typo must fail");
        let msg = err.to_string();
        assert!(msg.contains("typo_field"), "{msg}");
        assert!(msg.contains("fix:"), "{msg}");
        assert!(HarnessConfig::from_toml("[security.dependencies]\nnope = 1\n").is_err());
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("30s"), Ok(Duration::from_secs(30)));
        assert_eq!(parse_duration("10m"), Ok(Duration::from_secs(600)));
        assert_eq!(parse_duration(" 2h "), Ok(Duration::from_secs(7200)));
        assert!(parse_duration("10").is_err());
        assert!(parse_duration("0s").is_err());
        assert!(parse_duration("tenm").is_err());
        assert!(parse_duration("5d").is_err());
    }

    #[test]
    fn memory_sizes_parse() {
        assert_eq!(parse_memory("512MB"), Ok(512 << 20));
        assert_eq!(parse_memory("2gb"), Ok(2 << 30));
        assert_eq!(parse_memory("64KB"), Ok(64 << 10));
        assert!(parse_memory("512").is_err());
        assert!(parse_memory("0MB").is_err());
        assert!(parse_memory("lots").is_err());
    }

    #[test]
    fn validate_reports_every_problem_with_a_fix() {
        let cfg = HarnessConfig::from_toml(
            r#"
            [project]
            base_ref = ""
            [sandbox]
            provider = "podman"
            max_memory = "huge"
            timeout = "10"
            max_cpu_time = "0s"
            [validation]
            commands = [""]
            [agents]
            strict_mode = true
            "#,
        )
        .expect("parses");
        let problems = cfg.validate();
        let fields: Vec<_> = problems.iter().map(|p| p.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "sandbox.provider",
                "sandbox.max_memory",
                "sandbox.timeout",
                "sandbox.max_cpu_time",
                "project.base_ref",
                "validation",
                "agents.strict_mode",
            ]
        );
        assert!(problems.iter().all(|p| !p.fix.is_empty()));
        assert!(problems[0].to_string().contains("fix:"));
    }

    #[test]
    fn docker_needs_an_image() {
        let cfg = HarnessConfig::from_toml("[sandbox]\nimage = \"\"\n").expect("parses");
        assert_eq!(cfg.validate()[0].field, "sandbox.image");
        let wasm = HarnessConfig::from_toml("[sandbox]\nprovider = \"wasmtime\"\nimage = \"\"\n")
            .expect("parses");
        assert!(wasm.validate().is_empty());
    }
}
