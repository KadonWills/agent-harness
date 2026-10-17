//! Dependency policy for newly introduced dependencies (PRD §4.5).
//!
//! Manifests understood: `package.json` (npm), `requirements*.txt` and
//! `pyproject.toml` (PyPI), `Cargo.toml` (crates.io), `go.mod` (Go). A new
//! dependency is blocked when it matches a deny glob, misses a non-empty
//! allow list, sits one edit away from a popular package (typosquat), or
//! matches an offline OSV advisory.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use harness_core::config::DependencyPolicyConfig;
use harness_core::glob;
use harness_core::manifest::DependencyFinding;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ecosystem {
    Npm,
    PyPI,
    Cargo,
    Go,
}

impl Ecosystem {
    /// The manifest's ecosystem name, as written to `AgentManifest.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "pypi",
            Ecosystem::Cargo => "cargo",
            Ecosystem::Go => "go",
        }
    }

    /// The OSV `affected[].package.ecosystem` value.
    fn osv(self) -> &'static str {
        match self {
            Ecosystem::Npm => "npm",
            Ecosystem::PyPI => "PyPI",
            Ecosystem::Cargo => "crates.io",
            Ecosystem::Go => "Go",
        }
    }

    fn popular(self) -> &'static str {
        match self {
            Ecosystem::Npm => include_str!("popular/npm.txt"),
            Ecosystem::PyPI => include_str!("popular/pypi.txt"),
            Ecosystem::Cargo => include_str!("popular/cargo.txt"),
            Ecosystem::Go => include_str!("popular/go.txt"),
        }
    }

    /// Canonical form for comparing names (PEP 503 for PyPI).
    fn normalize(self, name: &str) -> String {
        match self {
            Ecosystem::PyPI => name.to_lowercase().replace(['_', '.'], "-"),
            Ecosystem::Cargo => name.replace('_', "-"),
            _ => name.to_string(),
        }
    }
}

/// The ecosystem of a manifest file, by file name.
pub fn manifest_ecosystem(path: &str) -> Option<Ecosystem> {
    let name = Path::new(path).file_name()?.to_str()?;
    match name {
        "package.json" => Some(Ecosystem::Npm),
        "pyproject.toml" => Some(Ecosystem::PyPI),
        "Cargo.toml" => Some(Ecosystem::Cargo),
        "go.mod" => Some(Ecosystem::Go),
        n if n.starts_with("requirements") && n.ends_with(".txt") => Some(Ecosystem::PyPI),
        _ => None,
    }
}

/// Dependency name -> version spec declared in a manifest. Unparseable
/// manifests yield nothing (the sandbox step will fail on them anyway).
pub fn extract(path: &str, content: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let name = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    match manifest_ecosystem(path) {
        Some(Ecosystem::Npm) => {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
                for section in [
                    "dependencies",
                    "devDependencies",
                    "optionalDependencies",
                    "peerDependencies",
                ] {
                    if let Some(deps) = json.get(section).and_then(|d| d.as_object()) {
                        for (k, v) in deps {
                            out.insert(k.clone(), v.as_str().unwrap_or_default().to_string());
                        }
                    }
                }
            }
        }
        Some(Ecosystem::PyPI) if name == "pyproject.toml" => {
            if let Ok(doc) = content.parse::<toml::Table>() {
                pyproject(&doc, &mut out);
            }
        }
        Some(Ecosystem::PyPI) => {
            for line in content.lines() {
                let line = line.split('#').next().unwrap_or_default().trim();
                if !line.is_empty() && !line.starts_with('-') {
                    if let Some((n, spec)) = requirement(line) {
                        out.insert(n, spec);
                    }
                }
            }
        }
        Some(Ecosystem::Cargo) => {
            if let Ok(doc) = content.parse::<toml::Table>() {
                cargo(&doc, &mut out);
            }
        }
        Some(Ecosystem::Go) => go_mod(content, &mut out),
        None => {}
    }
    out
}

/// PEP 508 requirement -> (name, version spec).
fn requirement(line: &str) -> Option<(String, String)> {
    let end = line
        .find(|c: char| !(c.is_ascii_alphanumeric() || "._-".contains(c)))
        .unwrap_or(line.len());
    let name = &line[..end];
    if name.is_empty() {
        return None;
    }
    let rest = line[end..].trim();
    let rest = match rest.strip_prefix('[') {
        Some(r) => r.split_once(']').map_or("", |(_, after)| after).trim(),
        None => rest,
    };
    let spec = rest.split(';').next().unwrap_or_default().trim();
    Some((name.to_string(), spec.to_string()))
}

fn pyproject(doc: &toml::Table, out: &mut BTreeMap<String, String>) {
    let mut add_strings = |list: Option<&toml::Value>| {
        for item in list.and_then(|v| v.as_array()).into_iter().flatten() {
            if let Some((n, spec)) = item.as_str().and_then(requirement) {
                out.insert(n, spec);
            }
        }
    };
    let project = doc.get("project");
    add_strings(project.and_then(|p| p.get("dependencies")));
    if let Some(groups) = project
        .and_then(|p| p.get("optional-dependencies"))
        .and_then(|g| g.as_table())
    {
        for list in groups.values() {
            add_strings(Some(list));
        }
    }
    let poetry = doc.get("tool").and_then(|t| t.get("poetry"));
    let mut tables: Vec<&toml::Value> = poetry
        .and_then(|p| p.get("dependencies"))
        .into_iter()
        .collect();
    if let Some(groups) = poetry
        .and_then(|p| p.get("group"))
        .and_then(|g| g.as_table())
    {
        tables.extend(groups.values().filter_map(|g| g.get("dependencies")));
    }
    for table in tables.into_iter().filter_map(|t| t.as_table()) {
        for (k, v) in table {
            if k != "python" {
                out.insert(k.clone(), version_of(v));
            }
        }
    }
}

fn version_of(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

fn cargo(doc: &toml::Table, out: &mut BTreeMap<String, String>) {
    const SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut tables: Vec<&toml::Value> = SECTIONS.iter().filter_map(|s| doc.get(*s)).collect();
    if let Some(ws) = doc.get("workspace").and_then(|w| w.get("dependencies")) {
        tables.push(ws);
    }
    for target in doc
        .get("target")
        .and_then(|t| t.as_table())
        .into_iter()
        .flat_map(|t| t.values())
    {
        tables.extend(SECTIONS.iter().filter_map(|s| target.get(*s)));
    }
    for table in tables.into_iter().filter_map(|t| t.as_table()) {
        for (key, v) in table {
            // `foo = { package = "real-name" }` renames a dependency.
            let name = v
                .get("package")
                .and_then(|p| p.as_str())
                .unwrap_or(key)
                .to_string();
            if v.get("path").is_none() && v.get("workspace").is_none() {
                out.insert(name, version_of(v));
            }
        }
    }
}

fn go_mod(content: &str, out: &mut BTreeMap<String, String>) {
    let mut in_block = false;
    for line in content.lines() {
        let line = line.split("//").next().unwrap_or_default().trim();
        let spec = if in_block {
            if line == ")" {
                in_block = false;
                continue;
            }
            line
        } else if line == "require (" {
            in_block = true;
            continue;
        } else if let Some(rest) = line.strip_prefix("require ") {
            rest
        } else {
            continue;
        };
        let mut parts = spec.split_whitespace();
        if let (Some(module), Some(version)) = (parts.next(), parts.next()) {
            out.insert(module.to_string(), version.to_string());
        }
    }
}

/// Dependencies present in `after` but not in `before`.
pub fn new_dependencies(path: &str, before: &str, after: &str) -> Vec<(String, String)> {
    let old = extract(path, before);
    extract(path, after)
        .into_iter()
        .filter(|(name, _)| !old.contains_key(name))
        .collect()
}

/// Optimal-string-alignment distance (Levenshtein plus transpositions).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// The popular package `name` imitates, if it is one edit away from one.
pub fn typosquat_target(eco: Ecosystem, name: &str) -> Option<&'static str> {
    let wanted = eco.normalize(name);
    if wanted.chars().count() < 4 {
        return None;
    }
    let popular = || {
        eco.popular()
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
    };
    if popular().any(|p| eco.normalize(p) == wanted) {
        return None;
    }
    popular().find(|p| edit_distance(&eco.normalize(p), &wanted) == 1)
}

/// An offline OSV advisory entry, reduced to what matching needs.
#[derive(Debug, Clone)]
struct Advisory {
    id: String,
    summary: String,
    ecosystem: String,
    name: String,
    versions: BTreeSet<String>,
}

/// Advisories loaded from a directory of OSV JSON files.
#[derive(Debug, Clone, Default)]
pub struct AdvisoryDb {
    entries: Vec<Advisory>,
}

impl AdvisoryDb {
    /// Load every `*.json` file under `dir` (recursively).
    pub fn load(dir: &Path) -> Result<Self, Error> {
        let mut files = Vec::new();
        collect_json(dir, &mut files).map_err(|e| {
            Error::Rules(format!(
                "cannot read advisory_db {}: {e}\nfix: point security.dependencies.advisory_db \
                 at a directory of OSV JSON files (e.g. an unzipped osv.dev export)",
                dir.display()
            ))
        })?;
        files.sort();
        let mut entries = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let str_of = |v: &serde_json::Value, k: &str| {
                v.get(k)
                    .and_then(|x| x.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            for affected in json
                .get("affected")
                .and_then(|a| a.as_array())
                .into_iter()
                .flatten()
            {
                let package = affected.get("package").cloned().unwrap_or_default();
                entries.push(Advisory {
                    id: str_of(&json, "id"),
                    summary: str_of(&json, "summary"),
                    ecosystem: str_of(&package, "ecosystem"),
                    name: str_of(&package, "name"),
                    versions: affected
                        .get("versions")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect(),
                });
            }
        }
        Ok(Self { entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// An advisory hit for `name@spec`. Malicious-package (`MAL-`) entries
    /// match any version; others need an exact pin listed as affected.
    fn find(&self, eco: Ecosystem, name: &str, spec: &str) -> Option<&Advisory> {
        let wanted = eco.normalize(name);
        let pinned = exact_version(spec);
        self.entries.iter().find(|a| {
            a.ecosystem == eco.osv()
                && eco.normalize(&a.name) == wanted
                && (a.id.starts_with("MAL-")
                    || pinned.as_ref().is_some_and(|v| a.versions.contains(v)))
        })
    }
}

fn collect_json(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_json(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            out.push(path);
        }
    }
    Ok(())
}

/// The version an exact pin names (`1.2.3`, `=1.2.3`, `==1.2.3`, `v1.2.3`).
fn exact_version(spec: &str) -> Option<String> {
    let s = spec.trim();
    let s = s
        .strip_prefix("==")
        .or_else(|| s.strip_prefix('='))
        .unwrap_or(s)
        .trim();
    let looks_exact = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
        && s.trim_start_matches('v')
            .starts_with(|c: char| c.is_ascii_digit());
    looks_exact.then(|| s.to_string())
}

/// A configured policy, ready to evaluate.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub typosquat_check: bool,
    pub advisories: AdvisoryDb,
}

impl Policy {
    /// Build from config; `root` resolves a relative `advisory_db`.
    pub fn from_config(config: &DependencyPolicyConfig, root: &Path) -> Result<Self, Error> {
        let advisories = if config.advisory_db.trim().is_empty() {
            AdvisoryDb::default()
        } else {
            AdvisoryDb::load(&root.join(&config.advisory_db))?
        };
        Ok(Self {
            allow: config.allow.clone(),
            deny: config.deny.clone(),
            typosquat_check: config.typosquat_check,
            advisories,
        })
    }

    /// Findings for the dependencies `file` adds between two versions.
    pub fn evaluate(&self, file: &str, before: &str, after: &str) -> Vec<DependencyFinding> {
        let Some(eco) = manifest_ecosystem(file) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, spec) in new_dependencies(file, before, after) {
            let finding = |reason: String| DependencyFinding {
                ecosystem: eco.as_str().to_string(),
                name: name.clone(),
                file: file.to_string(),
                reason,
            };
            let allowed = glob::any(&self.allow, &name);
            if let Some(p) = self.deny.iter().find(|p| glob::matches(p, &name)) {
                out.push(finding(format!(
                    "matches deny pattern `{p}`; fix: remove it, or change security.dependencies.deny"
                )));
                continue;
            }
            if !self.allow.is_empty() && !allowed {
                out.push(finding(
                    "not on the dependency allow list; fix: add it to \
                     security.dependencies.allow after review"
                        .to_string(),
                ));
                continue;
            }
            if self.typosquat_check && !allowed {
                if let Some(target) = typosquat_target(eco, &name) {
                    out.push(finding(format!(
                        "one edit away from the popular package `{target}` (possible \
                         typosquat); fix: use `{target}`, or add `{name}` to \
                         security.dependencies.allow if it is intended"
                    )));
                    continue;
                }
            }
            if let Some(adv) = self.advisories.find(eco, &name, &spec) {
                out.push(finding(format!(
                    "advisory {}: {}; fix: pick a version the advisory does not cover",
                    adv.id, adv.summary
                )));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_every_ecosystem() {
        let pkg = r#"{"dependencies":{"react":"^18"},"devDependencies":{"vitest":"1.0.0"}}"#;
        let deps = extract("web/package.json", pkg);
        assert_eq!(deps.get("vitest").map(String::as_str), Some("1.0.0"));
        assert_eq!(deps.len(), 2);

        let req = "requests==2.31.0  # http\n-r base.txt\nuvicorn[standard]>=0.20; python_version>'3.8'\n\n";
        let deps = extract("requirements-dev.txt", req);
        assert_eq!(deps.get("requests").map(String::as_str), Some("==2.31.0"));
        assert_eq!(deps.get("uvicorn").map(String::as_str), Some(">=0.20"));

        let py = "[project]\ndependencies = [\"httpx>=0.27\"]\n[project.optional-dependencies]\ndev = [\"pytest\"]\n[tool.poetry.dependencies]\npython = \"^3.11\"\nrich = { version = \"13.0\" }\n";
        let deps = extract("pyproject.toml", py);
        let names: Vec<_> = deps.keys().cloned().collect();
        assert_eq!(names, ["httpx", "pytest", "rich"]);

        let cargo = "[dependencies]\nserde = \"1\"\nlocal = { path = \"../x\" }\nalias = { package = \"tokio\", version = \"1\" }\n[target.'cfg(unix)'.dev-dependencies]\nnix = \"0.29\"\n[workspace.dependencies]\nanyhow = \"1\"\n";
        let names: Vec<_> = extract("Cargo.toml", cargo).keys().cloned().collect();
        assert_eq!(names, ["anyhow", "nix", "serde", "tokio"]);

        let gomod = "module x\n\nrequire github.com/a/b v1.0.0\nrequire (\n\tgolang.org/x/net v0.20.0 // indirect\n)\n";
        let names: Vec<_> = extract("go.mod", gomod).keys().cloned().collect();
        assert_eq!(names, ["github.com/a/b", "golang.org/x/net"]);

        assert!(extract("README.md", "anything").is_empty());
        assert!(extract("package.json", "{not json").is_empty());
    }

    #[test]
    fn typosquats_are_one_edit_from_popular_names() {
        assert_eq!(typosquat_target(Ecosystem::Npm, "lodahs"), Some("lodash"));
        assert_eq!(
            typosquat_target(Ecosystem::Npm, "expresss"),
            Some("express")
        );
        assert_eq!(
            typosquat_target(Ecosystem::PyPI, "reqeusts"),
            Some("requests")
        );
        assert_eq!(
            typosquat_target(Ecosystem::Cargo, "serde-jsn"),
            Some("serde_json"),
            "`-` and `_` are equivalent on crates.io"
        );
        assert_eq!(
            typosquat_target(Ecosystem::Cargo, "srde-jsn"),
            None,
            "two edits"
        );
        assert_eq!(
            typosquat_target(Ecosystem::Npm, "lodash"),
            None,
            "the real thing"
        );
        assert_eq!(
            typosquat_target(Ecosystem::PyPI, "Python_Dateutil"),
            None,
            "PEP 503"
        );
        assert_eq!(
            typosquat_target(Ecosystem::Npm, "preact"),
            None,
            "popular itself"
        );
        assert_eq!(typosquat_target(Ecosystem::Npm, "pq"), None, "too short");
        assert_eq!(edit_distance("ab", "ba"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    fn policy(allow: &[&str], deny: &[&str]) -> Policy {
        Policy {
            allow: allow.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|s| s.to_string()).collect(),
            typosquat_check: true,
            advisories: AdvisoryDb::default(),
        }
    }

    fn reasons(p: &Policy, before: &str, after: &str) -> Vec<(String, String)> {
        p.evaluate("package.json", before, after)
            .into_iter()
            .map(|f| (f.name, f.reason))
            .collect()
    }

    #[test]
    fn policy_blocks_new_dependencies_only() {
        let before = r#"{"dependencies":{"lodahs":"1"}}"#;
        let after = r#"{"dependencies":{"lodahs":"1","expresss":"4","left-pad":"1","zod":"3"}}"#;
        let found = reasons(&policy(&[], &["left-*"]), before, after);
        let names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["expresss", "left-pad"],
            "lodahs predates the change"
        );
        assert!(found[0].1.contains("typosquat") && found[0].1.contains("fix:"));
        assert!(found[1].1.contains("deny pattern `left-*`"));
    }

    #[test]
    fn allow_lists_gate_and_exempt() {
        let after = r#"{"dependencies":{"zod":"3","expresss":"1"}}"#;
        let found = reasons(&policy(&["zod", "expresss"], &[]), "{}", after);
        assert!(
            found.is_empty(),
            "allowed names skip the typosquat check: {found:?}"
        );
        let found = reasons(&policy(&["zod"], &[]), "{}", after);
        assert_eq!(found.len(), 1);
        assert!(found[0].1.contains("allow list"));
    }

    #[test]
    fn offline_osv_advisories() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("npm")).expect("mkdir");
        std::fs::write(
            dir.path().join("npm/MAL-2024-1.json"),
            r#"{"id":"MAL-2024-1","summary":"Malicious code in evil-pkg","affected":[{"package":{"ecosystem":"npm","name":"evil-pkg"}}]}"#,
        )
        .expect("write");
        std::fs::write(
            dir.path().join("GHSA-x.json"),
            r#"{"id":"GHSA-x","summary":"Prototype pollution","affected":[{"package":{"ecosystem":"PyPI","name":"Some_Lib"},"versions":["1.2.3"]}]}"#,
        )
        .expect("write");
        std::fs::write(dir.path().join("broken.json"), "{").expect("write");

        let cfg = DependencyPolicyConfig {
            advisory_db: dir.path().display().to_string(),
            ..DependencyPolicyConfig::default()
        };
        let p = Policy::from_config(&cfg, Path::new("/")).expect("load");
        assert_eq!(p.advisories.len(), 2);

        let npm = p.evaluate(
            "package.json",
            "{}",
            r#"{"dependencies":{"evil-pkg":"^9"}}"#,
        );
        assert!(npm[0].reason.starts_with("advisory MAL-2024-1"));
        let pinned = p.evaluate("requirements.txt", "", "some-lib==1.2.3\n");
        assert_eq!(pinned.len(), 1);
        let ranged = p.evaluate("requirements.txt", "", "some-lib>=1.0\n");
        assert!(ranged.is_empty(), "ranges are not resolved offline");

        let missing = DependencyPolicyConfig {
            advisory_db: "does/not/exist".into(),
            ..DependencyPolicyConfig::default()
        };
        let err = Policy::from_config(&missing, dir.path()).expect_err("missing dir");
        assert!(err.to_string().contains("fix:"));
    }

    #[test]
    fn exact_versions() {
        assert_eq!(exact_version("==1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(exact_version("=1.0.0-rc.1").as_deref(), Some("1.0.0-rc.1"));
        assert_eq!(exact_version("v0.20.0").as_deref(), Some("v0.20.0"));
        assert_eq!(exact_version("^1.2"), None);
        assert_eq!(exact_version(">=1"), None);
        assert_eq!(exact_version("latest"), None);
    }
}
