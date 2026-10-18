# Documentation

Start with the row that matches what you are here to do.

| I want to… | Read |
|---|---|
| Understand what AgentHarness is for | [PRD](PRD.md) |
| Install it and gate my first agent change | [README quick start](../README.md#quick-start) |
| Run the gate in CI | [GitHub Action guide](guides/github-action.md) |
| Use the terminal or web UI | [Local UIs guide](guides/local-uis.md) |
| See what is built and what is next | [Roadmap](ROADMAP.md) |
| Understand how the crates fit together | [Architecture](ARCHITECTURE.md) |
| Make my first contribution | [CONTRIBUTING](../CONTRIBUTING.md), then [adding a language](guides/adding-a-language.md) |
| Propose a change to a contract | [RFCs](rfcs/README.md) |
| Know why something was decided | [Architecture decision records](adr/README.md) |
| Get help or ask a question | [SUPPORT](../SUPPORT.md) |
| Report a vulnerability | [SECURITY](../SECURITY.md) — privately, never in an issue |
| Triage, review, merge, or release | [MAINTAINING](MAINTAINING.md) |

## Layout

```
docs/
  PRD.md            what and why; the product's source of truth
  ROADMAP.md        milestones and their status
  ARCHITECTURE.md   crates, data flow, extension points
  MAINTAINING.md    triage, labels, review, merging, releases
  guides/           task-oriented how-tos for users and contributors
  rfcs/             proposals that change a public contract
  adr/              decisions already made, and why
```

RFCs and ADRs differ in timing: an RFC is a proposal, discussed on its PR
and merged once accepted; an ADR records a decision, including ones that
never needed an RFC (language choice, parsing library).

## Writing docs

- Guides are task-first: the reader should be able to follow one top to
  bottom and end with something working.
- Every command shown must run as written against the current `main`.
- Errors quoted in docs must match what the binary prints, including the
  `fix:` line.
- Doc-only PRs use the `docs` type and scope: `docs(docs): …`.
