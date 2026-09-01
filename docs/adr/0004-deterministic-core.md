# ADR-0004: Deterministic core — no LLM calls in the gate

Status: Accepted

## Context
A guardrail that itself behaves probabilistically cannot be trusted or
debugged: the same diff must always produce the same verdict. LLM-backed
features (generated E2E tests, auto-fix suggestions, agent refinement
loops) are valuable but non-reproducible and require API keys.

## Decision
The core gate (`harness run` pass/fail/halt) is fully deterministic and
makes zero network calls of its own. LLM-backed capabilities ship as
optional plugins (v0.5 plugin API) with bring-your-own-key, and plugin
results are advisory unless the user explicitly promotes them to gating.

## Consequences
+ Reproducible CI verdicts; the core runs in air-gapped environments.
+ A clean seam for the commercial layer and community plugins.
- Some headline features (intent-aware test generation) are not in the
  default install; docs must set that expectation clearly.
