# ADR-0005: Crate naming is unresolved; the semver gate is disabled until it is

Status: Open — decision required before v0.1.0 is published

## Context
The workspace publishes four crates: `harness-cli`, `harness-core`,
`harness-ast`, and `harness-runtime`. Three of those four names are already
taken on crates.io by unrelated projects:

| Crate | crates.io status |
|---|---|
| `harness-cli` | taken — "Precise and reproducible benchmarking", ~13k downloads |
| `harness-core` | taken — shared types for the `@agent-sh/harness-*` tools |
| `harness-runtime` | taken — "Tokio execution engine for the Harness SDK" |
| `harness-ast` | available |

This is not a future problem. It broke a CI gate on the repository's first
day: `cargo-semver-checks` diffs the working tree against the *published*
crate of the same name, so it downloaded a stranger's `harness-runtime 0.1.0`
and reported `MiddlewareChain`, `TokenCounter`, `HarnessRuntime`, `RunResult`,
`StateTransition`, `RuntimeConfig`, and `trait Middleware` as breaking
removals. None of those items exist in this project. It then hard-errored
because `harness-ast` is not in the registry at all.

`README.md`'s quick start — `cargo install harness-cli` — currently installs
an unrelated benchmarking tool.

The surrounding identity is also contested: `github.com/agentharness` is an
existing org this project does not control, and `agentharness.dev` is
registered to a third party (it serves a "Private preview" placeholder and has
no MX record, which is why `SECURITY.md` and `CODE_OF_CONDUCT.md` route
reports through GitHub rather than email). The `agentharness-*` crate
namespace on crates.io is, by contrast, entirely free.

## Decision
Deferred — this needs an owner decision, not a default. The options:

1. **Rename to `agentharness-*`.** The whole namespace is available on
   crates.io. Does not resolve the GitHub org or domain, which belong to
   someone else.
2. **Pick an uncontested name** for the project as a whole, and take the
   matching crates, org, and domain together.
3. **Keep `harness-*` and never publish to crates.io** — distribute only via
   `cargo install --git` and release binaries. Viable, but forfeits the
   discovery path the README promises.

Until this is settled, two things are true and are encoded in the repository:

- `.github/workflows/semver.yml` is disabled (`workflow_dispatch` only). It
  cannot produce a meaningful verdict while it is comparing against crates
  owned by other people.
- The README quick start describes target UX for an unreleased tool and must
  not be followed literally.

## Consequences
+ CI stops reporting fabricated API breakage on every dependency PR.
+ The naming problem is recorded where contributors will find it, instead of
  being rediscovered by whoever first runs `cargo publish`.
- The project has no reserved identity on crates.io, so the names remain
  claimable by others until the decision is made. This argues for deciding
  sooner rather than later.
- Renaming later is a breaking change for anyone who installed from git in
  the interim, and touches every path in the workspace.

Re-enable the `pull_request` trigger in `semver.yml` in the same PR that
publishes v0.1.0 under the final names, and update this ADR's status.
