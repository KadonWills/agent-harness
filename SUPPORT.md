# Getting help

| You have… | Go to |
|---|---|
| A question about using or configuring harness | [Discussions → Q&A](https://github.com/KadonWills/agent-harness/discussions/categories/q-a) |
| An idea, or a change you want to sound out before an RFC | [Discussions → Ideas](https://github.com/KadonWills/agent-harness/discussions/categories/ideas) |
| Something that is broken | [A bug report](https://github.com/KadonWills/agent-harness/issues/new?template=bug_report.yml) |
| A security vulnerability | [Private vulnerability report](https://github.com/KadonWills/agent-harness/security/advisories/new) — never a public issue |
| Something you built with harness | [Discussions → Show and tell](https://github.com/KadonWills/agent-harness/discussions/categories/show-and-tell) |

Before asking, two commands answer most questions:

```bash
harness check          # what is wrong with the config or sandbox, and the fix
harness check --json   # the same, to paste into a bug report or discussion
```

Every error harness prints ends with a `fix:` line. If one doesn't, or the
fix doesn't work, that is a bug — please report it.

This is a volunteer-run, pre-alpha project. There is no paid support, and
response times are best effort; see [MAINTAINING](docs/MAINTAINING.md#response-times)
for what we aim for.
