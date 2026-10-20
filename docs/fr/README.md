<!-- translated-from: docs/README.md@6cf97db -->
[English](../README.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Documentation

Commencez par la ligne qui correspond à ce que vous êtes venu faire.

| Je veux… | Lire |
|---|---|
| Comprendre à quoi sert AgentHarness | [PRD](../PRD.md) (en anglais) |
| L'installer et valider ma première modification d'agent | [Démarrage rapide du README](../../README.fr.md#démarrage-rapide) |
| Exécuter la barrière de validation (gate) en CI | [Guide de la GitHub Action](guides/github-action.md) |
| Utiliser l'interface terminal ou web | [Guide des interfaces locales](guides/local-uis.md) |
| Voir ce qui est fait et ce qui vient ensuite | [Feuille de route](../ROADMAP.md) (en anglais) |
| Comprendre comment les crates s'articulent | [Architecture](../ARCHITECTURE.md) (en anglais) |
| Faire ma première contribution | [CONTRIBUTING](CONTRIBUTING.md), puis [ajouter un langage](guides/adding-a-language.md) |
| Proposer une modification d'un contrat | [RFC](../rfcs/README.md) (en anglais) |
| Savoir pourquoi une décision a été prise | [Registre des décisions d'architecture](../adr/README.md) (en anglais) |
| Obtenir de l'aide ou poser une question | [SUPPORT](SUPPORT.md) |
| Signaler une vulnérabilité | [SECURITY](SECURITY.md) — en privé, jamais dans une issue |
| Trier, relire, fusionner ou publier une version | [MAINTAINING](../MAINTAINING.md) (en anglais) |

## Organisation

```
docs/
  PRD.md            what and why; the product's source of truth
  ROADMAP.md        milestones and their status
  ARCHITECTURE.md   crates, data flow, extension points
  MAINTAINING.md    triage, labels, review, merging, releases
  guides/           task-oriented how-tos for users and contributors
  rfcs/             proposals that change a public contract
  adr/              decisions already made, and why
  fr/               traductions françaises des pages destinées aux utilisateurs
```

Les RFC et les ADR diffèrent par leur moment : une RFC est une proposition,
discutée sur sa PR et fusionnée une fois acceptée ; un ADR consigne une
décision, y compris celles qui n'ont jamais nécessité de RFC (choix du
langage, bibliothèque d'analyse syntaxique).

## Traductions

- La version anglaise fait foi ; en cas de divergence, c'est elle qui prévaut.
- Les traductions françaises couvrent les pages destinées aux utilisateurs
  (README, guides, contribution, support, sécurité, Code de conduite). Les
  documents de conception (PRD, feuille de route, architecture, RFC, ADR,
  MAINTAINING) restent en anglais.
- Chaque page traduite indique en en-tête le commit source dont elle est issue
  (`<!-- translated-from: <chemin>@<commit> -->`).
- `scripts/check-translations.sh` liste les pages dont la source anglaise a
  changé depuis ce commit, et qui doivent donc être mises à jour.

## Rédiger la documentation

- Les guides sont orientés tâche : le lecteur doit pouvoir en suivre un du
  début à la fin et obtenir un résultat fonctionnel.
- Chaque commande présentée doit s'exécuter telle quelle sur la branche `main`
  actuelle.
- Les erreurs citées dans la documentation doivent correspondre exactement à
  ce qu'affiche le binaire, ligne `fix:` comprise.
- Les PR ne touchant que la documentation utilisent le type et la portée
  `docs` : `docs(docs): …`.
