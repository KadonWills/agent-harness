//! Secret scanning over added lines, with gitleaks-compatible rules.
//!
//! Supported subset of the gitleaks rule format: `id`, `description`,
//! `regex`, `secretGroup`, `entropy`, `keywords`, `path`, per-rule
//! `[rules.allowlist]` / `[[rules.allowlists]]` (`regexes`, `stopwords`,
//! `paths`), and the global `[allowlist]` / `[[allowlists]]`. Path-only
//! rules (no `regex`) and other keys are ignored. A line containing
//! `gitleaks:allow` or `harness:allow-secret` is never reported.

use std::collections::BTreeSet;

use harness_core::manifest::SecretFinding;
use regex::Regex;
use serde::Deserialize;

use crate::{AddedLine, Error};

const BUILTIN: &str = include_str!("builtin-secrets.toml");
const INLINE_ALLOW: &[&str] = &["gitleaks:allow", "harness:allow-secret"];

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawFile {
    #[serde(default)]
    allowlist: Option<RawAllow>,
    #[serde(default)]
    allowlists: Vec<RawAllow>,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    regex: Option<String>,
    #[serde(default)]
    secret_group: usize,
    #[serde(default)]
    entropy: Option<f64>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    allowlist: Option<RawAllow>,
    #[serde(default)]
    allowlists: Vec<RawAllow>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAllow {
    #[serde(default)]
    regexes: Vec<String>,
    #[serde(default)]
    stopwords: Vec<String>,
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct Allow {
    regexes: Vec<Regex>,
    stopwords: Vec<String>,
    paths: Vec<Regex>,
}

impl Allow {
    fn compile(raw: impl IntoIterator<Item = RawAllow>, rule: &str) -> Result<Self, Error> {
        let mut allow = Allow::default();
        for r in raw {
            allow.regexes.extend(compile_all(&r.regexes, rule)?);
            allow.paths.extend(compile_all(&r.paths, rule)?);
            allow
                .stopwords
                .extend(r.stopwords.iter().map(|s| s.to_lowercase()));
        }
        Ok(allow)
    }

    fn permits(&self, path: &str, secret: &str) -> bool {
        let lower = secret.to_lowercase();
        self.paths.iter().any(|re| re.is_match(path))
            || self.regexes.iter().any(|re| re.is_match(secret))
            || self.stopwords.iter().any(|w| lower.contains(w))
    }
}

fn compile(pattern: &str, rule: &str) -> Result<Regex, Error> {
    Regex::new(pattern).map_err(|e| {
        Error::Rules(format!(
            "rule `{rule}`: {e}\nfix: correct the regex (gitleaks rules use RE2 syntax; \
             look-arounds are unsupported)"
        ))
    })
}

fn compile_all(patterns: &[String], rule: &str) -> Result<Vec<Regex>, Error> {
    patterns.iter().map(|p| compile(p, rule)).collect()
}

#[derive(Debug, Clone)]
struct Rule {
    id: String,
    description: String,
    regex: Regex,
    secret_group: usize,
    entropy: Option<f64>,
    keywords: Vec<String>,
    path: Option<Regex>,
    allow: Allow,
}

/// A compiled rule set.
#[derive(Debug, Clone, Default)]
pub struct Scanner {
    rules: Vec<Rule>,
    allow: Allow,
}

impl Scanner {
    /// The built-in rules (see `builtin-secrets.toml`).
    pub fn builtin() -> Self {
        // The embedded file is covered by tests; failing here is a build bug.
        Self::from_gitleaks_toml(BUILTIN).unwrap_or_default()
    }

    /// Parse a gitleaks-format rules file.
    pub fn from_gitleaks_toml(text: &str) -> Result<Self, Error> {
        let raw: RawFile = toml::from_str(text).map_err(|e| {
            Error::Rules(format!(
                "{e}\nfix: the file must be a gitleaks config ([[rules]] with id and regex)"
            ))
        })?;
        let allow = Allow::compile(raw.allowlist.into_iter().chain(raw.allowlists), "global")?;
        let mut rules = Vec::new();
        for r in raw.rules {
            let Some(pattern) = r.regex.filter(|p| !p.is_empty()) else {
                continue; // path-only rules are outside the supported subset
            };
            rules.push(Rule {
                regex: compile(&pattern, &r.id)?,
                path: r.path.as_deref().map(|p| compile(p, &r.id)).transpose()?,
                allow: Allow::compile(r.allowlist.into_iter().chain(r.allowlists), &r.id)?,
                keywords: r.keywords.iter().map(|k| k.to_lowercase()).collect(),
                description: r.description,
                secret_group: r.secret_group,
                entropy: r.entropy,
                id: r.id,
            });
        }
        Ok(Self { rules, allow })
    }

    /// Add `other`'s rules and allowlist; a rule with an existing id
    /// replaces it.
    pub fn extend(&mut self, other: Scanner) {
        for rule in other.rules {
            self.rules.retain(|r| r.id != rule.id);
            self.rules.push(rule);
        }
        let Allow {
            regexes,
            stopwords,
            paths,
        } = other.allow;
        self.allow.regexes.extend(regexes);
        self.allow.stopwords.extend(stopwords);
        self.allow.paths.extend(paths);
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Findings for `lines`, sorted and deduplicated per rule, path and line.
    pub fn scan(&self, lines: &[AddedLine]) -> Vec<SecretFinding> {
        let mut found = BTreeSet::new();
        for line in lines {
            if INLINE_ALLOW.iter().any(|m| line.text.contains(m)) {
                continue;
            }
            let lower = line.text.to_lowercase();
            for rule in &self.rules {
                if !rule.keywords.is_empty() && !rule.keywords.iter().any(|k| lower.contains(k)) {
                    continue;
                }
                if rule.path.as_ref().is_some_and(|p| !p.is_match(&line.path)) {
                    continue;
                }
                let hit = rule.regex.captures_iter(&line.text).any(|caps| {
                    let secret = if rule.secret_group > 0 {
                        caps.get(rule.secret_group)
                    } else {
                        caps.iter().skip(1).flatten().find(|m| !m.is_empty())
                    }
                    .or_else(|| caps.get(0))
                    .map_or("", |m| m.as_str());
                    !secret.is_empty()
                        && rule.entropy.is_none_or(|min| entropy(secret) >= min)
                        && !rule.allow.permits(&line.path, secret)
                        && !self.allow.permits(&line.path, secret)
                });
                if hit {
                    found.insert(SecretFinding {
                        rule_id: rule.id.clone(),
                        description: rule.description.clone(),
                        path: line.path.clone(),
                        line: line.line,
                    });
                }
            }
        }
        found.into_iter().collect()
    }
}

/// Shannon entropy in bits per character.
pub fn entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0usize;
    for c in s.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
        total += 1;
    }
    if total == 0 {
        return 0.0;
    }
    counts
        .values()
        .map(|&n| {
            let p = n as f64 / total as f64;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fake credentials are assembled at runtime so this source file never
    // contains a full match (the repository itself is gitleaks-scanned).
    fn aws_key() -> String {
        ["AKIA", "Z7VRSQ5TJS2GVAXR"].concat()
    }

    fn line(path: &str, n: u32, text: &str) -> AddedLine {
        AddedLine {
            path: path.into(),
            line: n,
            text: text.into(),
        }
    }

    fn ids(scanner: &Scanner, lines: &[AddedLine]) -> Vec<(String, u32)> {
        scanner
            .scan(lines)
            .into_iter()
            .map(|f| (f.rule_id, f.line))
            .collect()
    }

    #[test]
    fn builtin_rules_load() {
        let s = Scanner::builtin();
        assert!(s.rule_count() >= 15, "{}", s.rule_count());
        assert!(Scanner::from_gitleaks_toml(BUILTIN).is_ok());
    }

    #[test]
    fn finds_planted_credentials() {
        let s = Scanner::builtin();
        let ghp = ["ghp_", "8vKq2LmN4pR6sT0uW3xY5zA7bC9dE1fG2hJ4"].concat();
        let pem = ["-----BEGIN RSA ", "PRIVATE KEY-----"].concat();
        let generic = ["api_key = \"", "q8Zr4LmT2vXw9KpN6bYc", "\""].concat();
        let lines = [
            line("config.ts", 3, &format!("const k = \"{}\";", aws_key())),
            line("ci.sh", 1, &format!("export GH={ghp}")),
            line("id_rsa", 1, &pem),
            line("settings.py", 9, &generic),
            line("ok.ts", 1, "const answer = 42;"),
        ];
        assert_eq!(
            ids(&s, &lines),
            [
                ("aws-access-token".into(), 3),
                ("generic-api-key".into(), 9),
                ("github-pat".into(), 1),
                ("private-key".into(), 1),
            ]
        );
        let findings = s.scan(&lines);
        assert_eq!(findings[0].path, "config.ts");
        assert!(
            !format!("{findings:?}").contains(&aws_key()),
            "never record the secret"
        );
    }

    #[test]
    fn placeholders_low_entropy_and_inline_allows_pass() {
        let s = Scanner::builtin();
        let lines = [
            line("a", 1, "api_key = \"your_api_key_goes_here\""),
            line("a", 2, "password = \"${DATABASE_PASSWORD}\""),
            line("a", 3, "token = \"aaaaaaaaaaaaaaaaaaaa\""),
            line("a", 4, &format!("k = \"{}\" # gitleaks:allow", aws_key())),
            line(
                "a",
                5,
                &format!("k = \"{}\" # harness:allow-secret", aws_key()),
            ),
        ];
        assert!(s.scan(&lines).is_empty(), "{:?}", s.scan(&lines));
    }

    #[test]
    fn custom_gitleaks_rules_merge_and_override() {
        let custom = r#"
            title = "team rules"
            [[rules]]
            id = "acme-internal-token"
            description = "ACME token"
            regex = '''acme_[a-z0-9]{12}'''
            keywords = ["acme_"]
            tags = ["internal"]
            [rules.allowlist]
            paths = ['''^fixtures/''']

            [[rules]]
            id = "path-only"
            path = '''\.pem$'''
        "#;
        let mut s = Scanner::builtin();
        let before = s.rule_count();
        s.extend(Scanner::from_gitleaks_toml(custom).expect("valid"));
        assert_eq!(s.rule_count(), before + 1, "path-only rule skipped");
        let token = ["acme_", "a1b2c3d4e5f6"].concat();
        let lines = [
            line("src/x.py", 1, &token),
            line("fixtures/x.py", 1, &token),
        ];
        assert_eq!(ids(&s, &lines), [("acme-internal-token".into(), 1)]);
    }

    #[test]
    fn bad_rules_are_explained() {
        let err = Scanner::from_gitleaks_toml("[[rules]]\nid = \"x\"\nregex = '(?<=a)b'\n")
            .expect_err("look-behind unsupported");
        assert!(err.to_string().contains("fix:"));
        assert!(Scanner::from_gitleaks_toml("[[rules]]\nregex = 1").is_err());
    }

    #[test]
    fn entropy_matches_known_values() {
        assert_eq!(entropy(""), 0.0);
        assert_eq!(entropy("aaaa"), 0.0);
        assert!((entropy("ab") - 1.0).abs() < 1e-9);
        assert!(entropy("q8Zr4LmT2vXw9KpN6bYc") > 4.0);
    }
}
