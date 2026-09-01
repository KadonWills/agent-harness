# Governance

## Current phase: BDFL-with-maintainers

The project is young. The founding maintainer has final say, exercised
sparingly; day-to-day decisions happen in PRs and RFCs in the open.

## Roles

- **Contributor** — anyone with a merged PR.
- **Maintainer** — triage, review, and merge rights over one or more
  crates (see MAINTAINERS.md and CODEOWNERS). Nominated by an existing
  maintainer after sustained quality contributions; confirmed by lazy
  consensus (one week, no objections).
- **Founding maintainer** — tie-breaker; owner of releases and security
  process until those are delegated.

## Decisions

- Code-level: PR review, one maintainer approval + green gate.
- Contract-level (config schema, manifest, public traits, CLI surface):
  RFC required, accepted by maintainer consensus.
- Roadmap-level: discussed in the open (Discussions), decided by
  maintainers, recorded in docs/ROADMAP.md.

## Evolution

When there are ≥ 3 active non-founder maintainers, this document gets an
RFC to move to a steering-group model. The open core stays MIT OR
Apache-2.0 permanently; no relicensing to source-available.
