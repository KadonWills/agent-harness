# Running the gate in GitHub Actions

**English** | [Français](../fr/guides/github-action.md)

The repository root is a composite action. It builds `harness` from source
(nothing is published yet, see ADR-0005), runs `harness run` against the PR's
base branch, and uploads `AgentManifest.json` as an artifact. A blocked gate
fails the job (exit 1); a harness error fails it too (exit 2).

```yaml
# .github/workflows/agentharness.yml
name: AgentHarness
on: pull_request
permissions:
  contents: read
jobs:
  gate:
    runs-on: ubuntu-latest        # needs Docker: Linux runners have it
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0          # the gate diffs against the base branch
      - uses: KadonWills/agent-harness@main
        with:
          agent: ${{ github.event.pull_request.user.login }}
```

## Inputs

| Input | Default | Meaning |
|---|---|---|
| `agent` | `github-actions` | Checked against `agents.allowed` in `harness.toml`. |
| `base` | `origin/<PR base>` | Ref to diff against. |
| `working-directory` | `.` | A directory inside the repository to check; `harness.toml` is read from its git root. |
| `upload-manifest` | `true` | Upload `AgentManifest.json` as the `agent-manifest` artifact. |

## Caveats

- **Docker is required** for the default `docker` provider. GitHub's Linux
  runners have it; macOS and Windows runners do not.
- **`fetch-depth: 0`** (or at least the base branch) must be fetched, or the
  merge base cannot be computed and the run exits 2 with a fix message.
- Pin the action to a commit SHA rather than `@main` for supply-chain
  safety, as OpenSSF Scorecard recommends.

## Local hooks

`harness init --hooks` installs a `pre-push` hook that runs `harness run`
before every push. It refuses to overwrite a hook it did not write unless
you pass `--force`; delete `.git/hooks/pre-push` to opt out.
