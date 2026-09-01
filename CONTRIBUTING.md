# Contributing to AgentHarness

Thanks for helping build the guardrail layer for agentic engineering.
This document is written for humans **and** for AI coding agents — if you
are an agent, follow it exactly; the CI gate enforces most of it
automatically and will tell you precisely what to fix.

## TL;DR

```bash
git clone https://github.com/KadonWills/agent-harness
cd agent-harness
cargo build --workspace
cargo test --workspace
./scripts/check.sh        # run the exact checks CI runs, locally
```

Every PR must pass the automated gate (see "What CI enforces" below).
Green gate + one maintainer approval = merge.

## Ways to contribute

| Contribution | Where to start | Typical size |
|---|---|---|
| Language parser | docs/guides/adding-a-language.md | < 100 lines |
| Bug fix | issues labeled `bug` | any |
| Docs | anything unclear you just hit | any |
| New feature | **open an RFC first** (docs/rfcs/) for anything touching harness.toml, the manifest, public traits, or CLI surface | RFC → PR |
| Good first issue | issues labeled `good-first-issue` | small |

## Development workflow

1. **Fork and branch.** Branch from `main`; name it `feat/...`, `fix/...`,
   `docs/...`, `chore/...`.
2. **Write the change.** Keep PRs focused — one logical change per PR.
   Large mixed PRs are asked to split (the size bot labels anything
   > 600 changed lines `size/XL` and a maintainer will likely ask).
3. **Add tests.** Code changes without tests are not merged unless the PR
   explains why none are possible. Parser changes require fixture pairs.
4. **Run `./scripts/check.sh`** — it mirrors CI exactly (fmt, clippy,
   tests, docs). If it passes locally, the CI gate will pass.
5. **Commit with Conventional Commits + DCO sign-off** (both enforced):

   ```
   feat(ast): add go parser

   Maps go grammar node kinds to AstChange events.

   Signed-off-by: Your Name <you@example.com>
   ```

   Use `git commit -s` to add the sign-off. Allowed types: `feat`, `fix`,
   `docs`, `test`, `refactor`, `perf`, `chore`, `ci`, `build`. Scope is
   the crate short name (`cli`, `core`, `ast`, `runtime`) or area (`docs`,
   `ci`).
6. **Open the PR.** The template asks what/why/how-tested; fill it in.
   PR titles must themselves be valid conventional commit headers — they
   become the squash-merge commit.

## What CI enforces (the automated gate)

Every PR runs through `.github/workflows/`; all of these are required
checks — a PR cannot merge with any of them red:

| Check | Workflow | Fails when |
|---|---|---|
| Formatting | ci.yml | `cargo fmt --check` differs |
| Lints | ci.yml | any `cargo clippy` warning (`-D warnings`) |
| Tests | ci.yml | any test fails on Linux/macOS/Windows |
| Docs | ci.yml | `cargo doc` warnings (broken intra-doc links) |
| MSRV | ci.yml | build fails on the pinned minimum Rust version |
| PR title | pr-validation.yml | title is not a valid conventional commit |
| DCO | pr-validation.yml | any commit lacks `Signed-off-by` |
| Coverage | coverage.yml | patch coverage < 70% or project coverage drops |
| Licenses/advisories | security.yml | cargo-deny finds a disallowed license, yanked crate, or advisory |
| Secrets | security.yml | gitleaks finds a credential in the diff |
| API breakage | semver.yml | cargo-semver-checks finds an unacknowledged breaking change in a published crate (label `breaking-change` + RFC link to acknowledge) |

The gate is strict on purpose: this project's whole thesis is that
automated validation beats human vigilance. We apply it to ourselves first.

## Contributions from AI agents

Agent-authored PRs are welcome and expected. Requirements:

- The PR description must disclose the agent used (e.g. "Authored with
  Claude Code") and a human must be the PR author of record, responsible
  for the submission.
- Agents: read `harness.toml`-style error output from CI and self-correct;
  do not open a new PR per attempt — push to the same branch.
- Wholesale generated changes with no tests and no issue reference are
  closed without review.

## RFC process

Changes to any public contract — `harness.toml` schema, `AgentManifest.json`,
the `LanguageParser`/`SandboxBackend` traits, CLI commands/flags/exit codes —
start as an RFC: copy `docs/rfcs/0000-template.md` to the next number, open
a PR, discussion happens on that PR. Accepted RFCs merge before their
implementation PRs.

## Code style

- `rustfmt.toml` and clippy settings in the workspace are the style guide;
  there is no separate prose style doc to memorize.
- `unsafe` is denied workspace-wide; if you believe you need it, that's an
  RFC.
- Errors shown to users/agents must say what is wrong **and** what to do
  about it.

## Licensing

Dual-licensed MIT OR Apache-2.0. By contributing (with DCO sign-off) you
agree your contribution is licensed the same way. No CLA.

## Getting help

- GitHub Discussions for questions and design talk
- Issues for bugs and concrete proposals
- Security reports: see SECURITY.md — never open a public issue for a
  vulnerability
