<!-- translated-from: README.md@0e47168437fe -->
[English](README.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# AgentHarness

[![CI](https://github.com/KadonWills/agent-harness/actions/workflows/ci.yml/badge.svg)](https://github.com/KadonWills/agent-harness/actions/workflows/ci.yml)
[![Security](https://github.com/KadonWills/agent-harness/actions/workflows/security.yml/badge.svg)](https://github.com/KadonWills/agent-harness/actions/workflows/security.yml)
[![Release](https://img.shields.io/github/v/release/KadonWills/agent-harness)](https://github.com/KadonWills/agent-harness/releases/latest)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/KadonWills/agent-harness/badge)](https://scorecard.dev/viewer/?uri=github.com/KadonWills/agent-harness)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#licence)

**Des garde-fous pour les agents de code autonomes.** AgentHarness s'intercale
entre les agents de code IA (Claude Code, Cline, Cursor, Copilot Workspace, …)
et votre dépôt git : il exécute les modifications générées par les agents dans
un bac à sable (sandbox), les compare structurellement au niveau de l'AST,
détecte les conflits sémantiques entre agents concurrents, et bloque les
secrets et les dépendances non vérifiées — le tout **avant** que le code
n'arrive en revue humaine.

> État : **pré-alpha.** Les jalons v0.1 à v0.4 (barrière,
> diff structurel, conflits multi-agents, sécurité) sont implémentés
> conformément à [RFC-0001](docs/rfcs/0001-v0.1-v0.4-contracts.md) (en
> anglais) et publiés dans la version
> [v0.1.0](https://github.com/KadonWills/agent-harness/releases/latest).
> Voir la [feuille de route](docs/ROADMAP.md) (en anglais).

## Pourquoi

Les flottes d'agents écrivent du code plus vite que les humains ne peuvent le
vérifier. Aujourd'hui, la première exécution d'une modification produite par
un agent a souvent lieu pendant *votre* revue, voire pire, en CI après la
fusion. AgentHarness transforme les questions « ce code a-t-il déjà été
exécuté, est-il structurellement cohérent, et est-il sûr ? » en une barrière
de validation (gate) automatique et déterministe.

## Fonctionnement

```
agent edits ──► harness run ──► sandbox (no network) ──► AST diff ──► gate
                                                                       │
                              AgentManifest.json ◄─────────── pass / block
```

- **Cœur déterministe** — la barrière n'appelle jamais de LLM : même diff, même verdict.
- **Bac à sable zero-trust** — trafic sortant refusé par défaut ; les tentatives sont journalisées.
- **Diff structurel** — basé sur Tree-sitter, avec des événements de modification indépendants du langage.
- **Conscient du multi-agents** — repère les modifications qui cassent le code ensemble, invisibles pour une simple fusion.
- **Lisible par les agents** — configuration, erreurs et rapports conçus pour que les LLM puissent s'auto-corriger.

## Démarrage rapide

Téléchargez un binaire précompilé depuis la
[dernière version](https://github.com/KadonWills/agent-harness/releases/latest)
(Linux x86_64, macOS arm64/x86_64, Windows x86_64 ; interfaces terminal et web
incluses) :

```bash
target=aarch64-apple-darwin   # ou x86_64-apple-darwin, x86_64-unknown-linux-gnu
curl -fsSL "https://github.com/KadonWills/agent-harness/releases/latest/download/harness-$target.tar.gz" | tar xz
sudo mv harness /usr/local/bin/
```

Chaque archive est accompagnée d'un fichier `.sha256` et d'une provenance de
build signée
(`gh attestation verify harness-$target.tar.gz --repo KadonWills/agent-harness`).
Vous pouvez aussi compiler depuis les sources. Les crates ne sont pas publiées
sur crates.io : le `harness-cli` qu'on y trouve est un outil sans rapport (voir
[ADR-0005](docs/adr/0005-crate-naming.md), en anglais).

```bash
cargo install --locked --git https://github.com/KadonWills/agent-harness harness-cli
# ajoutez `--features wasmtime` pour le backend de bac à sable WASI (Rust 1.96+),
# `--features tui,web` pour les interfaces terminal et web
```

Ensuite, dans votre dépôt :

```bash
cd your-repo
harness init --hooks   # écrit harness.toml ; pre-commit = scan, pre-push = run
harness check          # valide la configuration et la disponibilité du bac à sable
harness run            # snapshot → bac à sable → diff AST → conflits → sécurité → AgentManifest.json
```

| Commande | Rôle |
|---|---|
| `harness init [--force] [--hooks]` | Détecte Node/Python/Rust/Go et écrit `harness.toml` avec vos commandes d'installation et de test |
| `harness check [--json]` | Signale les problèmes de configuration, une référence de base manquante ou un bac à sable indisponible, chacun avec un correctif |
| `harness run [--agent] [--intent\|--intent-file] [--base] [--manifest] [--json]` | La barrière complète ; écrit `.harness/AgentManifest.json` |
| `harness scan [--staged\|--base] [--json]` | Uniquement les secrets et la politique sur les nouvelles dépendances (le hook pre-commit) |
| `harness conflicts [--base] [--fix] [--json] [BRANCH...]` | Conflits sémantiques entre branches d'agents ; `--fix` applique les codemods de niveau 1 |
| `harness tui` | Interface terminal regroupant tout ce qui précède, plus l'historique des exécutions (`--features tui`) |
| `harness ui [--port] [--open]` | La même chose sous forme d'interface web locale sur 127.0.0.1, protégée par jeton (`--features web`) |

Codes de sortie : **0** succès, **1** la barrière a bloqué, **2** harness n'a
pas pu s'exécuter. En CI, utilisez la
[GitHub Action](docs/fr/guides/github-action.md).

La configuration se trouve dans [`harness.toml`](examples/harness.toml) (en anglais) :

```toml
[sandbox]
provider = "docker"
image = "node:22-bookworm-slim"
allow_egress = false        # les commandes de validation n'ont pas accès au réseau
allow_setup_egress = true   # l'installation des dépendances, si

[validation]
setup = ["npm ci"]
commands = ["npm test"]

[agents]
allowed = ["claude-code", "cline", "github-copilot"]
strict_mode = true

[conflicts]
branches = ["agent/*"]
```

## Langages pris en charge (diff structurel)

| Langage | Extensions | Feature |
|---|---|---|
| TypeScript / TSX | `.ts` `.mts` `.cts` `.tsx` | `lang-typescript` |
| JavaScript / JSX | `.js` `.jsx` `.mjs` `.cjs` | `lang-javascript` |
| Python | `.py` `.pyi` | `lang-python` |

En ajouter un se résume à une table de correspondance de moins de 100 lignes —
voir [le guide](docs/fr/guides/adding-a-language.md).

## Organisation du dépôt

```
crates/harness-cli       the `harness` binary and library (TUI, web UI)
crates/harness-core      config + AgentManifest contracts (dependency-light)
crates/harness-ast       Tree-sitter structural diff engine
crates/harness-runtime   sandbox backends (Docker, Wasmtime)
crates/harness-security  secret scanning + dependency policy
docs/                    PRD, architecture, ADRs, RFCs, guides
.github/                 the automated contribution gate
```

## Documentation

Le [plan de la documentation](docs/fr/README.md) vous oriente selon votre
besoin. À retenir :

- [Exigences produit](docs/PRD.md) (en anglais) · [Feuille de route](docs/ROADMAP.md) (en anglais)
- [Architecture](docs/ARCHITECTURE.md) (en anglais) · [Registre des décisions](docs/adr/README.md) (en anglais) · [RFC](docs/rfcs/README.md) (en anglais)
- [GitHub Action](docs/fr/guides/github-action.md) · [Interfaces terminal et web](docs/fr/guides/local-uis.md)
- [Contribuer](docs/fr/CONTRIBUTING.md) — y compris
  [l'ajout d'un analyseur de langage en moins de 100 lignes](docs/fr/guides/adding-a-language.md)
- [Politique de sécurité](docs/fr/SECURITY.md) · [Gouvernance](GOVERNANCE.md) (en anglais)

## Contribuer

Les PR — qu'elles viennent d'humains ou d'agents — passent par la même
barrière automatisée que celle dont ce projet fait la promotion : formatage,
lints, tests sur trois OS, couverture, conventional commits, DCO, audit des
licences et des avis de sécurité, détection de secrets et vérifications
semver. Si la barrière est au vert et qu'un mainteneur approuve, la PR est
fusionnée. Commencez par le [guide de contribution](docs/fr/CONTRIBUTING.md)
ou une issue
[`good first issue`](https://github.com/KadonWills/agent-harness/labels/good%20first%20issue).

## Communauté

- Questions et idées : [Discussions](https://github.com/KadonWills/agent-harness/discussions) — voir [SUPPORT](docs/fr/SUPPORT.md)
- Vulnérabilités : à signaler en privé, conformément à [SECURITY](docs/fr/SECURITY.md)
- [Code de conduite](docs/fr/CODE_OF_CONDUCT.md) · [Gouvernance](GOVERNANCE.md) (en anglais) · [Mainteneurs](MAINTAINERS.md) (en anglais)

## Licence

MIT OR Apache-2.0, au choix. Voir [LICENSE-MIT](LICENSE-MIT) (en anglais) et
[LICENSE-APACHE](LICENSE-APACHE) (en anglais).
