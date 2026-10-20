<!-- translated-from: docs/guides/local-uis.md@6cf97db -->
[English](../../guides/local-uis.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Interfaces terminal et web

`harness tui` et `harness ui` affichent sur un seul écran tout ce que la CLI
sait d'un dépôt :

- l'état de préparation ;
- les exécutions en cours et passées ;
- les détections de secrets et de dépendances ;
- les conflits entre branches d'agents ;
- la configuration.

Toutes deux appellent les mêmes fonctions que `harness check`, `run`, `scan`
et `conflicts` : elles ne peuvent donc jamais aboutir à un verdict différent
de celui de la CLI. La conception est décrite dans
[RFC-0002](../../rfcs/0002-local-uis.md) (en anglais).

## Installation

Les deux sont des fonctionnalités cargo optionnelles, ce qui garde la
compilation par défaut légère :

```bash
cargo install --locked --git https://github.com/KadonWills/agent-harness harness-cli \
  --features tui,web
```

Dans une compilation sans la fonctionnalité correspondante, la commande se
termine avec le code 2 et affiche une ligne `fix:` vous indiquant de
recompiler avec cette fonctionnalité.

## Écrans

| Écran | Équivalent | Actions |
|---|---|---|
| Status | `harness check` | actualiser |
| Run | `harness run` | lancer une exécution sur l'arbre de travail ; les étapes s'affichent à mesure qu'elles se terminent |
| History | `.harness/history/*.json` | ouvrir l'un des 50 derniers manifestes : motifs, étapes, `ast_changes`, détections |
| Findings | `harness scan` | actualiser |
| Conflicts | `harness conflicts` | appliquer les corrections de niveau 1 (Level 1) à la branche extraite |
| Config | `harness.toml` + validation | modifier et revalider |

Chaque `harness run` écrit `.harness/history/<session>.json`. Cela inclut les
exécutions lancées depuis la CLI, depuis les hooks git et depuis les
interfaces : l'écran History les affiche donc toutes. Seules les 50 plus
récentes sont conservées.

## Interface terminal

```bash
harness tui            # or: harness -C path/to/repo tui
```

| Touche | Action |
|---|---|
| `1`–`6`, `Tab`, `Shift-Tab` | changer d'écran |
| `r` | actualiser l'écran courant |
| `s` | (Run) lancer une exécution ; l'identifiant d'agent provient de `HARNESS_AGENT` |
| `↑` `↓`, `j` `k` | (History) sélectionner une exécution ; ailleurs, faire défiler |
| `PgUp` `PgDn` | faire défiler |
| `f` | (Conflicts) appliquer les corrections de niveau 1 |
| `e` | (Config) ouvrir `harness.toml` dans `$VISUAL`/`$EDITOR`, puis revalider |
| `q`, `Esc` | quitter |

Après une modification, l'écran Config liste chaque paramètre que la
modification a assoupli, par exemple `sandbox.allow_egress turned on`.

## Interface web

```bash
harness ui --open      # picks a free port and opens the browser
harness ui --port 7800 # fixed port; prints the URL instead
```

Le serveur peut lancer des exécutions dans le bac à sable (sandbox) et
réécrire `harness.toml` ; il est donc verrouillé :

- **Boucle locale uniquement.** Il écoute sur `127.0.0.1` et aucune option ne
  permet d'écouter sur une autre adresse. Depuis une autre machine, utilisez
  `ssh -L 7800:127.0.0.1:7800 host`.
- **L'URL affichée contient un jeton à usage unique.** L'ouvrir définit un
  cookie `HttpOnly`, `SameSite=Strict` et retire le jeton de la barre
  d'adresse. Les scripts peuvent envoyer `Authorization: Bearer <token>` à la
  place. Redémarrer le serveur émet un nouveau jeton.
- **Les requêtes portant tout autre `Host` sont refusées**, ce qui bloque le
  DNS rebinding.
- **Les modifications exigent un en-tête personnalisé.** Toute requête qui
  modifie quelque chose doit porter un en-tête `X-Harness-Token`, ainsi qu'un
  `Origin` correspondant s'il est envoyé. Les autres sites web ne peuvent
  falsifier ni l'un ni l'autre.
- **Les enregistrements de configuration sont vérifiés.** Un enregistrement
  n'a lieu que si le texte est analysé et validé sans erreur. Une
  modification qui assouplit la barrière indique ce qu'elle assouplit et
  exige un second clic explicite sur « Save anyway » (enregistrer quand
  même).
- **Rien n'est chargé depuis Internet.** Ni CDN, ni polices, ni outil
  d'analytique.

### API JSON

Les pages reposent sur le même JSON que celui produit par les options
`--json` de la CLI ; les scripts peuvent donc utiliser l'API directement :

| Méthode | Chemin | Corps / résultat |
|---|---|---|
| GET | `/api/status` | `harness check --json` |
| GET | `/api/findings` | `harness scan --json` |
| GET | `/api/conflicts` | `harness conflicts --json` |
| POST | `/api/conflicts/fix` | idem, après application des corrections de niveau 1 |
| GET | `/api/history` | manifestes, du plus récent au plus ancien |
| GET | `/api/history/{session_id}` | un `AgentManifest.json` |
| GET | `/api/run` | état en direct de l'exécution lancée depuis l'interface |
| POST | `/api/run` | `{"agent": "id"}` → 202 ; 409 si une exécution est en cours |
| GET | `/api/config` | `{path, text, error, problems}` |
| POST | `/api/config` | `{"text": "...", "confirm_loosen": false}` → 200 enregistré, 409 confirmation requise, 422 invalide |

Les interfaces ne se terminent jamais avec le code 1. Bloquer est le rôle de
`harness run` et de `harness scan`. Elles se terminent avec le code 0 quand
vous quittez, et 2 si elles ne peuvent pas démarrer.
