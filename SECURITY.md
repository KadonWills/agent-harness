# Security Policy

AgentHarness is a security tool: it decides whether code written by an
autonomous agent may reach a repository. A flaw in it can let unsafe code
through, so we treat reports against it accordingly.

## Reporting a vulnerability

**Do not open a public issue, discussion, or pull request.** Use GitHub's
private vulnerability reporting: the
[**Report a vulnerability**](https://github.com/KadonWills/agent-harness/security/advisories/new)
button under the repository's Security tab. It is the only supported
channel. It opens a private thread with the maintainers and a draft
advisory in one place, and no email address is needed.

A useful report includes:

- the version or commit (`harness --version`, or the `main` SHA)
- the sandbox backend (`docker` or `wasmtime`) and the host OS
- the smallest `harness.toml`, repository, or agent output that reproduces it
- what an attacker gains: host code execution, network egress, a secret
  reaching a commit, a change passing the gate that should have been blocked
- whether you have shared it with anyone else

## What happens next

| Step | Target |
|---|---|
| Acknowledgment | within 72 hours |
| Triage verdict and severity (CVSS) | within 7 days |
| Fix or mitigation for high/critical issues | within 30 days of triage |
| Public disclosure | when a fix ships, or 90 days after the report, whichever is first |

Fixes ship as a GitHub Security Advisory with a CVE requested through
GitHub. You are credited in it unless you would rather not be. If we need
longer than 90 days, we will say why and agree a date with you.

## Scope

Especially interested in:

- sandbox escapes from the Docker or Wasmtime backends
- bypasses of egress denial (`allow_egress = false`)
- secret-scanner evasions: credentials that reach a commit undetected
- dependency-policy bypasses: a typosquat or advisory that passes
- ways a malicious repository, `harness.toml`, or crafted agent output can
  run code on the host running `harness`
- the local web UI (`harness ui`): reaching it from another origin or host,
  bypassing the token, or CSRF against its write endpoints
- a manifest that reports `Pass` for a change that should be `Blocked`

Out of scope:

- issues that require an attacker who already controls the host or the
  user's account
- `allow_egress = true` or other settings the user explicitly loosened
  (the web UI asks for confirmation before saving these)
- vulnerabilities in Docker, Wasmtime, or other dependencies themselves —
  report those upstream; a report that harness uses them unsafely is in scope
- missing hardening with no demonstrated impact, such as absent HTTP
  headers on the loopback-only web UI

## Safe harbor

We will not pursue or support legal action against research that is done
in good faith, stays within this policy, avoids privacy violations and
service disruption, uses only your own systems and test repositories, and
gives us reasonable time to fix the issue before disclosure.

## Supported versions

Nothing has been released yet. Until the first release, fixes land on
`main`. Pre-1.0, only the latest release receives fixes. Post-1.0, the
latest minor release of the current major does.

## How this repository is protected

- Every pull request runs cargo-deny (licenses and RustSec advisories),
  gitleaks, and GitHub dependency review, and must pass them to merge.
- Secret scanning with push protection, Dependabot alerts, and Dependabot
  security updates are enabled.
- Workflow tokens are read-only by default, and Actions cannot approve
  pull requests.
- `unsafe` Rust is denied across the workspace.
