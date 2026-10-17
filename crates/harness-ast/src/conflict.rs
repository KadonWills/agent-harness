//! Cross-branch semantic conflict detection (RFC-0001 §6).
//!
//! Each concurrent agent branch is a [`ChangeSet`]: its structural changes
//! per file. Rules compare every ordered pair of sets and flag collisions
//! that a textual merge cannot see, such as one branch removing a field
//! that another starts reading. Name-based and conservative by design.

use std::collections::{BTreeMap, BTreeSet};

use crate::AstChange;

/// One file's structural changes within a change set.
#[derive(Debug, Clone, Default)]
pub struct FileChanges {
    pub path: String,
    pub changes: Vec<AstChange>,
}

/// Everything one branch (or working tree) changed relative to its base.
#[derive(Debug, Clone, Default)]
pub struct ChangeSet {
    /// Label used in reports, e.g. the branch name.
    pub name: String,
    /// Every changed file, including ones no parser understands.
    pub files: Vec<FileChanges>,
}

/// How to resolve a Level 1 conflict.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fix {
    /// Rewrite identifier references `from` → `to` in these files of the
    /// second branch (see [`crate::codemod::rename_identifiers`]).
    RenameReferences {
        from: String,
        to: String,
        files: Vec<String>,
    },
    /// Run this command after merging.
    Command(String),
}

impl std::fmt::Display for Fix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fix::RenameReferences { from, to, files } => write!(
                f,
                "rename references `{from}` -> `{to}` in {} (harness conflicts --fix)",
                files.join(", ")
            ),
            Fix::Command(cmd) => write!(f, "after merging, run `{cmd}`"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Conflict {
    /// 1 = auto-fixable, 2 = halt and escalate.
    pub level: u8,
    pub kind: &'static str,
    pub symbol: String,
    /// `[a, b]`: `a` made the change that `b` collides with.
    pub branches: [String; 2],
    pub explanation: String,
    pub fix: Option<Fix>,
}

/// The facts each rule needs, extracted once per change set.
#[derive(Default)]
struct Summary {
    removed: BTreeSet<String>,
    renamed: BTreeMap<String, String>,
    removed_members: BTreeSet<(String, String)>,
    signature_changed: BTreeSet<String>,
    /// path -> qualified symbols changed in it.
    changed: BTreeMap<String, BTreeSet<String>>,
    /// identifier -> files that newly reference it.
    new_refs: BTreeMap<String, BTreeSet<String>>,
    defined: BTreeSet<String>,
    lockfiles: BTreeSet<String>,
}

const LOCKFILES: &[(&str, &str)] = &[
    ("package-lock.json", "npm install --package-lock-only"),
    ("pnpm-lock.yaml", "pnpm install --lockfile-only"),
    ("yarn.lock", "yarn install --mode update-lockfile"),
    ("bun.lock", "bun install --lockfile-only"),
    ("Cargo.lock", "cargo generate-lockfile"),
    ("poetry.lock", "poetry lock"),
    ("uv.lock", "uv lock"),
    ("Pipfile.lock", "pipenv lock"),
    ("go.sum", "go mod tidy"),
    ("Gemfile.lock", "bundle lock"),
    ("composer.lock", "composer update --lock"),
];

fn lockfile_command(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    LOCKFILES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, cmd)| *cmd)
}

/// `User.save` -> `save`: references are unqualified identifiers.
fn leaf(symbol: &str) -> String {
    symbol.rsplit('.').next().unwrap_or(symbol).to_string()
}

fn summarize(set: &ChangeSet) -> Summary {
    let mut s = Summary::default();
    for file in &set.files {
        if lockfile_command(&file.path).is_some() {
            s.lockfiles.insert(file.path.clone());
        }
        for change in &file.changes {
            match change {
                AstChange::FunctionRemoved(q) | AstChange::TypeRemoved(q) => {
                    s.removed.insert(leaf(q));
                }
                AstChange::FunctionRenamed { from, to } => {
                    s.renamed.insert(leaf(from), leaf(to));
                    s.defined.insert(leaf(to));
                }
                AstChange::MemberRemoved { owner, member } => {
                    s.removed_members.insert((owner.clone(), member.clone()));
                }
                AstChange::SignatureChanged(q) => {
                    s.signature_changed.insert(leaf(q));
                }
                AstChange::FunctionChanged(q) | AstChange::TypeChanged(q) => {
                    s.changed
                        .entry(file.path.clone())
                        .or_default()
                        .insert(q.clone());
                }
                AstChange::FunctionAdded(q) | AstChange::TypeAdded(q) => {
                    s.defined.insert(leaf(q));
                }
                AstChange::ReferenceAdded(name) => {
                    s.new_refs
                        .entry(name.clone())
                        .or_default()
                        .insert(file.path.clone());
                }
                _ => {}
            }
        }
    }
    s
}

fn files(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Compare every pair of change sets. Output is sorted and deduplicated.
pub fn detect(sets: &[ChangeSet]) -> Vec<Conflict> {
    let summaries: Vec<Summary> = sets.iter().map(summarize).collect();
    let mut out = BTreeSet::new();
    for (i, a) in summaries.iter().enumerate() {
        for (j, b) in summaries.iter().enumerate() {
            if i == j {
                continue;
            }
            let names = [sets[i].name.clone(), sets[j].name.clone()];
            let (an, bn) = (&names[0], &names[1]);
            let conflict = |level, kind, symbol: String, explanation, fix| Conflict {
                level,
                kind,
                symbol,
                branches: names.clone(),
                explanation,
                fix,
            };
            let refs = |name: &str| b.new_refs.get(name).filter(|_| !b.defined.contains(name));

            for x in &a.removed {
                if let Some(where_) = refs(x) {
                    out.insert(conflict(
                        2,
                        "removed-symbol",
                        x.clone(),
                        format!(
                            "`{bn}` adds references to `{x}` ({}), but `{an}` removes `{x}`; \
                             merged, those references break",
                            files(where_)
                        ),
                        None,
                    ));
                }
            }
            for (from, to) in &a.renamed {
                if let Some(where_) = refs(from) {
                    out.insert(conflict(
                        1,
                        "renamed-symbol",
                        from.clone(),
                        format!(
                            "`{an}` renames `{from}` to `{to}`, and `{bn}` adds references to \
                             the old name ({})",
                            files(where_)
                        ),
                        Some(Fix::RenameReferences {
                            from: from.clone(),
                            to: to.clone(),
                            files: where_.iter().cloned().collect(),
                        }),
                    ));
                }
            }
            for (owner, member) in &a.removed_members {
                // A same-named function in B (a GraphQL resolver for the
                // field, say) is not a replacement for the member.
                if let Some(where_) = b.new_refs.get(member) {
                    out.insert(conflict(
                        2,
                        "removed-member",
                        format!("{owner}.{member}"),
                        format!(
                            "`{an}` removes `{member}` from `{owner}`, and `{bn}` adds code \
                             reading `{member}` ({}): schema drift that no textual merge \
                             conflict will reveal",
                            files(where_)
                        ),
                        None,
                    ));
                }
            }
            for x in &a.signature_changed {
                if let Some(where_) = b.new_refs.get(x) {
                    out.insert(conflict(
                        2,
                        "signature-change",
                        x.clone(),
                        format!(
                            "`{an}` changes the signature of `{x}`, and `{bn}` adds calls to \
                             it ({}) written against the old signature",
                            files(where_)
                        ),
                        None,
                    ));
                }
            }
            if i < j {
                for (path, symbols) in &a.changed {
                    if let Some(theirs) = b.changed.get(path) {
                        for q in symbols.intersection(theirs) {
                            out.insert(conflict(
                                2,
                                "concurrent-edit",
                                q.clone(),
                                format!("both `{an}` and `{bn}` change `{q}` in {path}"),
                                None,
                            ));
                        }
                    }
                }
                for path in a.lockfiles.intersection(&b.lockfiles) {
                    let cmd = lockfile_command(path).unwrap_or("your package manager");
                    out.insert(conflict(
                        1,
                        "lockfile",
                        path.clone(),
                        format!("both `{an}` and `{bn}` modify {path}"),
                        Some(Fix::Command(cmd.to_string())),
                    ));
                }
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use AstChange::*;

    fn set(name: &str, files: &[(&str, Vec<AstChange>)]) -> ChangeSet {
        ChangeSet {
            name: name.into(),
            files: files
                .iter()
                .map(|(p, c)| FileChanges {
                    path: p.to_string(),
                    changes: c.clone(),
                })
                .collect(),
        }
    }

    fn kinds(conflicts: &[Conflict]) -> Vec<(u8, &str, &str)> {
        conflicts
            .iter()
            .map(|c| (c.level, c.kind, c.symbol.as_str()))
            .collect()
    }

    #[test]
    fn schema_drift_is_caught() {
        let a = set(
            "agent/rename",
            &[(
                "schema.ts",
                vec![
                    TypeChanged("User".into()),
                    MemberRemoved {
                        owner: "User".into(),
                        member: "email".into(),
                    },
                ],
            )],
        );
        let b = set(
            "agent/resolver",
            &[(
                "resolvers.ts",
                vec![
                    FunctionAdded("email".into()),
                    ReferenceAdded("email".into()),
                ],
            )],
        );
        let found = detect(&[a, b]);
        assert_eq!(kinds(&found), [(2, "removed-member", "User.email")]);
        assert_eq!(found[0].branches, ["agent/rename", "agent/resolver"]);
        assert!(found[0].explanation.contains("resolvers.ts"));
    }

    #[test]
    fn removed_renamed_and_signature_rules() {
        let a = set(
            "a",
            &[(
                "lib.py",
                vec![
                    FunctionRemoved("legacy".into()),
                    FunctionRenamed {
                        from: "fmt".into(),
                        to: "format_name".into(),
                    },
                    SignatureChanged("Store.load".into()),
                ],
            )],
        );
        let b = set(
            "b",
            &[(
                "app.py",
                vec![
                    ReferenceAdded("legacy".into()),
                    ReferenceAdded("fmt".into()),
                    ReferenceAdded("load".into()),
                ],
            )],
        );
        let found = detect(&[a, b]);
        assert_eq!(
            kinds(&found),
            [
                (1, "renamed-symbol", "fmt"),
                (2, "removed-symbol", "legacy"),
                (2, "signature-change", "load"),
            ]
        );
        assert_eq!(
            found[0].fix,
            Some(Fix::RenameReferences {
                from: "fmt".into(),
                to: "format_name".into(),
                files: vec!["app.py".into()],
            })
        );
    }

    #[test]
    fn redefining_the_name_is_not_a_conflict() {
        let a = set("a", &[("x.ts", vec![FunctionRemoved("helper".into())])]);
        let b = set(
            "b",
            &[(
                "y.ts",
                vec![
                    FunctionAdded("helper".into()),
                    ReferenceAdded("helper".into()),
                ],
            )],
        );
        assert!(detect(&[a, b]).is_empty());
    }

    #[test]
    fn concurrent_edits_and_lockfiles_are_reported_once() {
        let a = set(
            "a",
            &[
                ("svc.ts", vec![FunctionChanged("Repo.save".into())]),
                ("package-lock.json", vec![]),
            ],
        );
        let b = set(
            "b",
            &[
                ("svc.ts", vec![FunctionChanged("Repo.save".into())]),
                ("web/package-lock.json", vec![]),
                ("package-lock.json", vec![]),
            ],
        );
        let found = detect(&[a, b]);
        assert_eq!(
            kinds(&found),
            [
                (1, "lockfile", "package-lock.json"),
                (2, "concurrent-edit", "Repo.save"),
            ]
        );
        assert_eq!(
            found[0].fix.as_ref().map(ToString::to_string).as_deref(),
            Some("after merging, run `npm install --package-lock-only`")
        );
    }

    #[test]
    fn independent_changes_do_not_conflict() {
        let a = set("a", &[("a.ts", vec![FunctionAdded("x".into())])]);
        let b = set("b", &[("b.ts", vec![ReferenceAdded("y".into())])]);
        assert!(detect(&[a, b]).is_empty());
        assert!(detect(&[]).is_empty());
    }
}
