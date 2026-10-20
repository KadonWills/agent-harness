<!-- translated-from: CONTRIBUTING.md@fd496387f6e3 -->
[English](../../CONTRIBUTING.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Contribuer à AgentHarness

Merci de nous aider à construire la couche de garde-fous de l'ingénierie
agentique. Ce document s'adresse aux humains **et** aux agents de code IA :
si vous êtes un agent, suivez-le à la lettre ; la barrière de validation
(gate) de la CI en applique automatiquement l'essentiel et vous dira
précisément quoi corriger.

Les issues, les discussions et les descriptions de PR peuvent être rédigées
en français ou en anglais ; le code, les commentaires et les messages de
commit restent en anglais.

## En bref

```bash
git clone https://github.com/KadonWills/agent-harness
cd agent-harness
cargo build --workspace
cargo test --workspace
./scripts/check.sh        # exécute en local exactement les vérifications de la CI
```

Chaque PR doit passer la barrière automatisée (voir « Ce que la CI impose »
ci-dessous). Barrière au vert + approbation d'un mainteneur = fusion.

## Comment contribuer

| Contribution | Par où commencer | Taille habituelle |
|---|---|---|
| Parser de langage | [guides/adding-a-language.md](guides/adding-a-language.md) | < 100 lignes |
| Correction de bug | issues avec le label `bug` | variable |
| Documentation | tout ce qui vous a paru obscur | variable |
| Nouvelle fonctionnalité | **ouvrez d'abord une RFC** ([docs/rfcs/](../rfcs/) (en anglais)) pour tout ce qui touche à harness.toml, au manifeste, aux traits publics ou à l'interface de la CLI | RFC → PR |
| Bonne première issue | issues avec le label [`good first issue`](https://github.com/KadonWills/agent-harness/labels/good%20first%20issue) | petite |
| Aide bienvenue | issues avec le label [`help wanted`](https://github.com/KadonWills/agent-harness/labels/help%20wanted) | variable |

Vous ne savez pas par où commencer ? Posez la question dans les
[Discussions](https://github.com/KadonWills/agent-harness/discussions) :
il suffit de dire ce que vous savez et ce que vous aimeriez apprendre.

## Votre première contribution

1. Choisissez une issue et indiquez en commentaire que vous la prenez, pour
   éviter les doublons. Sans nouvelles de votre part pendant deux semaines,
   elle pourra être réattribuée.
2. Lisez [docs/ARCHITECTURE.md](../ARCHITECTURE.md) (en anglais) pour la
   crate que vous modifiez. [docs/README.md](README.md) recense le reste.
3. Installez une toolchain Rust via [rustup](https://rustup.rs) (le fichier
   `rust-toolchain.toml` du dépôt sélectionne la bonne version), ainsi que
   Docker si vous voulez exécuter les tests du bac à sable (sandbox) de bout
   en bout.
4. Ouvrez tôt une PR en brouillon (draft) si vous souhaitez un avis sur
   l'orientation avant de peaufiner.

## Processus de développement

1. **Forkez et créez une branche.** Partez de `main` ; nommez-la `feat/...`,
   `fix/...`, `docs/...`, `chore/...`.
2. **Écrivez la modification.** Gardez des PR ciblées : un changement
   logique par PR. Les grosses PR hétérogènes devront être découpées (le bot
   de taille applique le label `size/XL` au-delà de 600 lignes modifiées, et
   un mainteneur le demandera probablement).
3. **Ajoutez des tests.** Une modification de code sans tests n'est pas
   fusionnée, sauf si la PR explique pourquoi aucun n'est possible. Les
   modifications de parser exigent des paires de fixtures.
4. **Lancez `./scripts/check.sh`** : il reproduit exactement la CI (fmt,
   clippy, tests, docs). S'il passe en local, la barrière de la CI passera.
5. **Commitez au format Conventional Commits avec signature DCO
   (sign-off)** (les deux sont imposés). Les messages de commit restent en
   anglais, comme dans l'exemple :

   ```
   feat(ast): add go parser

   Maps go grammar node kinds to AstChange events.

   Signed-off-by: Your Name <you@example.com>
   ```

   Utilisez `git commit -s` pour ajouter la signature. Types autorisés :
   `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`, `ci`,
   `build`. Le scope est le nom court de la crate (`cli`, `core`, `ast`,
   `runtime`, `security`) ou le domaine (`docs`, `ci`, `deps`). Le sujet
   commence par une minuscule.

   Signature oubliée ? `git commit --amend -s --no-edit` pour le dernier
   commit, ou `git rebase --signoff main` pour tous, puis
   `git push --force-with-lease`.
6. **Ouvrez la PR.** Le modèle demande quoi / pourquoi / comment c'est
   testé : remplissez-le. Le titre de la PR doit lui-même être un en-tête de
   conventional commit valide : il devient le commit de la fusion squash.

## Ce que la CI impose (la barrière automatisée)

Chaque PR passe par `.github/workflows/`. La protection de branche exige
`Gate` (qui couvre le formatage, les lints, les tests, la documentation et
la MSRV), les vérifications du titre de PR et du DCO, cargo-deny, gitleaks
et la revue des dépendances : une PR ne peut pas être fusionnée si l'une
d'elles est au rouge.

| Vérification | Workflow | Échoue quand |
|---|---|---|
| Formatage | ci.yml | `cargo fmt --check` signale une différence |
| Lints | ci.yml | `cargo clippy` émet un avertissement (`-D warnings`) |
| Tests | ci.yml | un test échoue sous Linux/macOS/Windows |
| Documentation | ci.yml | `cargo doc` émet des avertissements (liens intra-doc cassés) |
| MSRV | ci.yml | la compilation échoue avec la version minimale de Rust épinglée |
| Titre de PR | pr-validation.yml | le titre n'est pas un conventional commit valide |
| DCO | pr-validation.yml | un commit n'a pas de `Signed-off-by` |
| Couverture | coverage.yml | couverture du patch < 70 % ou baisse de la couverture du projet ; **inactive tant que le secret `CODECOV_TOKEN` n'est pas défini**, voir la note dans `coverage.yml` |
| Licences/avis | security.yml | cargo-deny trouve une licence non autorisée, une crate retirée (yanked) ou un avis de sécurité |
| Secrets | security.yml | gitleaks trouve un identifiant dans le diff |
| Nouvelles dépendances | dependency-review.yml | une nouvelle dépendance a une vulnérabilité connue ou une licence hors de la liste autorisée de `deny.toml` |
| Test de fumée de l'Action | action-smoke.yml | la GitHub Action cesse de laisser passer une bonne modification ou de bloquer une mauvaise |
| Rupture d'API | semver.yml | cargo-semver-checks trouve un changement cassant non signalé ; **désactivé jusqu'à la première publication sur crates.io**, voir [ADR-0005](../adr/0005-crate-naming.md) (en anglais) |

La barrière est stricte à dessein : toute la thèse de ce projet est que la
validation automatisée l'emporte sur la vigilance humaine. Nous nous
l'appliquons d'abord à nous-mêmes.

## Contributions d'agents IA

Les PR rédigées par des agents sont bienvenues et attendues. Conditions :

- La description de la PR doit indiquer l'agent utilisé (par exemple
  « Authored with Claude Code ») et un humain doit être l'auteur de
  référence de la PR, responsable de la soumission.
- Agents : lisez les messages d'erreur de la CI (du type de ceux de
  `harness.toml`) et corrigez-vous ; n'ouvrez pas une nouvelle PR à chaque
  tentative, poussez sur la même branche.
- Les modifications générées en bloc, sans tests ni référence à une issue,
  sont fermées sans revue.

## Processus RFC

Toute modification d'un contrat public (schéma de `harness.toml`,
`AgentManifest.json`, traits `LanguageParser`/`SandboxBackend`, commandes,
flags et codes de sortie de la CLI) commence par une RFC : copiez
`docs/rfcs/0000-template.md` sous le numéro suivant et ouvrez une PR ; la
discussion a lieu sur cette PR. Les RFC acceptées sont fusionnées avant les
PR qui les implémentent. Le processus complet et l'index des RFC se trouvent
dans [docs/rfcs/README.md](../rfcs/README.md) (en anglais).

## Revue et fusion

Ce qui se passe après l'ouverture d'une PR (ordre des revues, délais de
réponse visés, fusion squash, fusion des PR empilées) est décrit dans
[docs/MAINTAINING.md](../MAINTAINING.md) (en anglais). En résumé : barrière
au vert, une approbation d'un code owner, conversations résolues, puis
fusion squash.

## Style de code

- `rustfmt.toml` et les réglages clippy du workspace font office de guide de
  style ; il n'y a pas de document de style séparé à mémoriser.
- `unsafe` est interdit dans tout le workspace ; si vous pensez en avoir
  besoin, c'est une RFC.
- Les erreurs présentées aux utilisateurs et aux agents doivent dire ce qui
  ne va pas **et** quoi faire pour y remédier.

## Licence

Double licence MIT OR Apache-2.0. En contribuant (avec signature DCO), vous
acceptez que votre contribution soit placée sous les mêmes licences. Pas de
CLA.

## Obtenir de l'aide

Voir [SUPPORT.md](SUPPORT.md). En résumé : les Discussions pour les
questions et les échanges de conception, les issues pour les bugs et les
propositions concrètes, et [SECURITY.md](SECURITY.md) pour les
vulnérabilités, jamais une issue publique.

Chaque participant est tenu de respecter le
[Code de conduite](CODE_OF_CONDUCT.md).
