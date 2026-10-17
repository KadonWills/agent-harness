//! Locating and loading `harness.toml`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use harness_core::config::HarnessConfig;

pub const CONFIG_FILE: &str = "harness.toml";

pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_FILE)
}

/// Read and parse `harness.toml` at the repository root. Semantic problems
/// are left to the caller (`check` reports them, `run` refuses to start).
pub fn load(root: &Path) -> Result<HarnessConfig> {
    let path = config_path(root);
    if !path.is_file() {
        bail!(
            "no {CONFIG_FILE} in {}\nfix: run `harness init` to create one",
            root.display()
        );
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    HarnessConfig::from_toml(&text).with_context(|| format!("invalid {}", path.display()))
}

/// Load and require a semantically valid config.
pub fn load_valid(root: &Path) -> Result<HarnessConfig> {
    let config = load(root)?;
    let problems = config.validate();
    if !problems.is_empty() {
        let list: Vec<String> = problems.iter().map(|p| format!("  - {p}")).collect();
        bail!(
            "{CONFIG_FILE} has {} problem(s):\n{}",
            problems.len(),
            list.join("\n")
        );
    }
    Ok(config)
}
