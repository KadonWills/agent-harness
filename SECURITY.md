# Security Policy

AgentHarness is a security tool; we treat reports against it accordingly.

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private vulnerability
reporting: the [**Report a vulnerability**](https://github.com/KadonWills/agent-harness/security/advisories/new)
button under the repository's Security tab. It is enabled on this
repository and is the only supported reporting channel — it gives us a
private thread with you and an advisory draft in one place.

You will get an acknowledgment within 72 hours and a triage verdict
within 7 days. We ask for 90 days of coordinated disclosure; credit is
given in the advisory unless you prefer otherwise.

## Scope

Especially interested in: sandbox escapes (Docker/Wasmtime backends),
egress-denial bypasses, secret-scanner evasions, and ways a malicious
repository or crafted agent output could execute code on the host running
`harness`.

## Supported versions

Pre-1.0: only the latest release receives fixes. Post-1.0: latest minor
of the current major.
