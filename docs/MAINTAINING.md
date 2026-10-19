# Maintaining AgentHarness

How issues, PRs, dependencies, and releases are handled. For contributors
this explains what happens after you open something; for maintainers it is
the checklist.

## Response times

Goals, not guarantees — this is a volunteer project:

| Item | First response |
|---|---|
| Security report | 72 hours (see [SECURITY](../SECURITY.md)) |
| New issue | triaged within 7 days |
| PR with a green gate | first review within 7 days |
| RFC | open for at least 7 days before a decision |

If something has waited longer, a polite ping on the thread is welcome.

## Triage

New issues arrive labeled `needs-triage`. Triage means:

1. Reproduce or ask for what is missing (`harness check --json` output
   usually is). Close duplicates with a link, using `duplicate`.
2. Add one area label (`area/cli`, `area/core`, `area/ast`,
   `area/runtime`, `area/security`, `area/docs`, `area/ci`).
3. If it changes a contract, add `contract-change` and point to the
   [RFC process](rfcs/README.md).
4. If a newcomer could do it in an afternoon with the pointers you leave
   in the issue, add `good first issue`. If it is wanted but no maintainer
   plans to do it, add `help wanted`.
5. Remove `needs-triage`.

Questions filed as issues are moved to Discussions → Q&A.

## Labels

| Label | Meaning | Applied by |
|---|---|---|
| `area/*` | Crate or area touched | labeler on PRs, triage on issues |
| `size/XS` … `size/XL` | Changed lines; XL is over 600 | size labeler |
| `bug` `enhancement` `documentation` `question` | Kind of issue | issue templates, triage |
| `needs-triage` | Not yet looked at | issue templates |
| `good first issue` `help wanted` | Open for contributors; also surfaces the issue on GitHub's contribute page | triage |
| `contract-change` | Touches a public contract; needs an RFC | labeler, triage |
| `rfc` | An RFC proposal | labeler |
| `breaking-change` | Acknowledged API break; skips the semver gate | maintainer only |
| `parser-request` | Request for a new language parser | issue template |
| `security` | Security-relevant change | triage |
| `blocked` | Waiting on a decision or upstream | triage |
| `agent-authored` | Authored with an AI coding agent | author or triage |
| `dependencies` | Dependabot updates | Dependabot |

## Reviewing

A PR is ready for review when the gate is green. Review for, in order:

1. **Contracts** — does it change `harness.toml`, the manifest, a public
   trait, or the CLI surface? If so, is there an accepted RFC it follows?
2. **Determinism** — no LLM calls, network, clock, or randomness in the
   gate path ([ADR-0004](adr/0004-deterministic-core.md)).
3. **Errors** — every new user-facing error says what is wrong and ends
   with a `fix:`.
4. **Tests** — they fail without the change.
5. **Scope** — one logical change. Ask for a split rather than reviewing
   an XL PR as-is.

Use GitHub's "Request changes" only for blockers; leave nits as comments
and approve.

## Merging

- **Squash merge only.** The PR title becomes the commit, so it must be a
  valid conventional commit (CI checks this).
- Branch protection on `main` requires: `Gate`, `Conventional PR title`,
  `DCO sign-off`, `Licenses & advisories (cargo-deny)`,
  `Secret scan (gitleaks)`, `New dependency review`, one approving review
  from a code owner, resolved conversations, and an up-to-date branch.
- Merged branches are deleted automatically.

### Stacked PRs

Squash merging a stack needs care, because the next PR still carries the
original commits of the one that was just squashed:

1. Before merging PR *N*, retarget PR *N+1* to `main`. If *N*'s branch is
   deleted first, GitHub **closes** *N+1* instead of retargeting it, and it
   can only be reopened by restoring the branch.
2. After merging *N*, rebase the rest of the stack onto `main`, dropping
   *N*'s commits, then force-push:

   ```bash
   git fetch origin
   git rebase --update-refs --onto origin/main <old tip of N> <top of stack>
   git push --force-with-lease origin <every remaining branch>
   ```

   Skipping this makes *N+1* conflict whenever it edits a file *N* added.

## Conduct reports

Reports arrive under **Moderation → Reported content** in the repository
settings, and through maintainers' GitHub profiles
([CODE_OF_CONDUCT](../CODE_OF_CONDUCT.md#how-to-report)).

1. Acknowledge privately within 72 hours. Never discuss a report in public
   threads.
2. A maintainer involved in the report hands it off. With one maintainer,
   point the reporter to GitHub's Trust & Safety instead.
3. Decide using the Covenant's enforcement ladder (correction, warning,
   temporary ban, permanent ban). Use GitHub's tools to act: hide comments,
   lock conversations, set interaction limits, or block users from the
   repository.
4. Keep a private note of the outcome so repeated behavior is visible.

## Dependencies

Dependabot opens grouped weekly PRs for Cargo and GitHub Actions.

- Minor and patch groups merge once the gate is green.
- Major bumps (for example `toml` 0.8 → 1.x) need a local
  `./scripts/check.sh`; fix any breakage on the Dependabot branch or in a
  follow-up PR, then merge.
- A new transitive licence must be on `deny.toml`'s allowlist. Do not widen
  the allowlist to admit one dependency — trim its features or replace it.
- Comment `@dependabot rebase` on a PR that has fallen behind `main`.
- GitHub Actions are pinned to full commit SHAs with the version in a
  trailing comment (`uses: actions/checkout@<sha> # v4.4.0`). Dependabot
  bumps both. Pin any new action the same way; a tag can be moved to
  malicious code, a SHA cannot.

## Releases

Releases are tag-driven and maintainer-run. Nothing is published to
crates.io until crate naming is settled ([ADR-0005](adr/0005-crate-naming.md)).

1. Curate `CHANGELOG.md`: move `[Unreleased]` entries under the new
   version, grouped from the conventional commits since the last tag.
2. Bump versions in `Cargo.toml` in a `chore(cli): release vX.Y.Z` PR.
3. After it merges, tag `main`: `git tag -a vX.Y.Z -m vX.Y.Z && git push origin vX.Y.Z`.
4. `release.yml` builds binaries for Linux, macOS (x86_64, arm64), and
   Windows and attaches them with SHA-256 checksums to a **draft** release,
   plus a signed build-provenance attestation that anyone can check with
   `gh attestation verify <file> -R KadonWills/agent-harness`.
5. Generate the notes in the draft (categories come from
   `.github/release.yml`), review, and publish.

## Repository settings

Kept in code where GitHub allows it: `.github/` (templates, CODEOWNERS,
labeler, Dependabot, release-note categories, workflows). The rest is set
in the GitHub UI and recorded here so it can be rebuilt:

- Features: Issues, Discussions (Announcements, General, Ideas, Polls,
  Q&A, Show and tell); no wiki.
- Merging: squash only, title from the PR, auto-merge allowed, "update
  branch" suggested, head branches deleted after merge.
- Commits made on the web must be signed off (DCO).
- Moderation: reported content accepted from all users, so conduct
  reports reach maintainers privately (Settings → Moderation options).
- Security: private vulnerability reporting, Dependabot alerts and
  security updates, secret scanning with push protection.
- Actions: default token is read-only and cannot approve PRs.
