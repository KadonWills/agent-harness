<!-- translated-from: docs/guides/github-action.md@ddf4263a95f9 -->
[English](../../guides/github-action.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Exécuter la barrière dans GitHub Actions

La racine du dépôt est une action composite. Elle compile `harness` depuis
les sources (rien n'est encore publié, voir ADR-0005), exécute `harness run`
par rapport à la branche de base de la PR et téléverse `AgentManifest.json`
comme artefact. Une barrière de validation (gate) bloquée fait échouer le job
(code de sortie 1) ; une erreur de harness le fait aussi échouer (code de
sortie 2).

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

## Entrées

| Entrée | Valeur par défaut | Signification |
|---|---|---|
| `agent` | `github-actions` | Vérifié par rapport à `agents.allowed` dans `harness.toml`. |
| `base` | `origin/<PR base>` | Référence à laquelle comparer. |
| `working-directory` | `.` | Un répertoire du dépôt à vérifier ; `harness.toml` est lu depuis la racine git de ce répertoire. |
| `upload-manifest` | `true` | Téléverse `AgentManifest.json` comme artefact `agent-manifest`. |

## Points d'attention

- **Docker est requis** pour le fournisseur par défaut `docker`. Les runners
  Linux de GitHub en disposent ; les runners macOS et Windows, non.
- **`fetch-depth: 0`** (ou au minimum la branche de base) doit être récupéré,
  sinon la base de fusion ne peut pas être calculée et l'exécution se termine
  avec le code 2 et un message indiquant comment corriger.
- Épinglez l'action sur le SHA d'un commit plutôt que sur `@main`, pour la
  sécurité de la chaîne d'approvisionnement, comme le recommande OpenSSF
  Scorecard.

## Hooks locaux

`harness init --hooks` installe un hook `pre-push` qui exécute `harness run`
avant chaque push. Il refuse d'écraser un hook qu'il n'a pas écrit, sauf si
vous passez `--force` ; supprimez `.git/hooks/pre-push` pour le désactiver.
