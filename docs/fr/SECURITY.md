<!-- translated-from: SECURITY.md@6cf97db -->
[English](../../SECURITY.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Politique de sécurité

AgentHarness est un outil de sécurité : il décide si du code écrit par un
agent autonome peut atteindre un dépôt. Une faille dans l'outil peut laisser
passer du code dangereux ; nous traitons donc les signalements qui le
concernent en conséquence.

## Signaler une vulnérabilité

**N'ouvrez pas d'issue, de discussion ni de pull request publique.**
Utilisez le signalement privé de vulnérabilités de GitHub : le bouton
[**Report a vulnerability**](https://github.com/KadonWills/agent-harness/security/advisories/new)
dans l'onglet Security du dépôt. C'est le seul canal pris en charge. Il
ouvre en un seul endroit un fil privé avec les mainteneurs et un brouillon
d'avis de sécurité, sans qu'aucune adresse e-mail soit nécessaire.

Un signalement utile comprend :

- la version ou le commit (`harness --version`, ou le SHA de `main`)
- le backend de bac à sable (sandbox) (`docker` ou `wasmtime`) et l'OS hôte
- le plus petit `harness.toml`, dépôt ou sortie d'agent qui reproduit le
  problème
- ce qu'y gagne un attaquant : exécution de code sur l'hôte, trafic sortant
  réseau, secret qui atteint un commit, modification qui passe la barrière
  de validation (gate) alors qu'elle aurait dû être bloquée
- si vous l'avez partagé avec quelqu'un d'autre

## Et ensuite

| Étape | Objectif |
|---|---|
| Accusé de réception | sous 72 heures |
| Verdict de tri et sévérité (CVSS) | sous 7 jours |
| Correctif ou atténuation pour les problèmes élevés/critiques | sous 30 jours après le tri |
| Divulgation publique | à la publication du correctif, ou 90 jours après le signalement, selon ce qui arrive en premier |

Les correctifs sont publiés sous forme d'avis de sécurité GitHub (GitHub
Security Advisory), avec un CVE demandé via GitHub. Vous y êtes crédité,
sauf si vous préférez ne pas l'être. S'il nous faut plus de 90 jours, nous
vous expliquerons pourquoi et conviendrons d'une date avec vous.

## Périmètre

Nous sommes particulièrement intéressés par :

- les évasions du bac à sable des backends Docker ou Wasmtime
- les contournements du blocage du trafic sortant (`allow_egress = false`)
- les évasions du scanner de secrets : des identifiants qui atteignent un
  commit sans être détectés
- les contournements de la politique de dépendances : un typosquatting ou
  un avis de sécurité qui passe
- les moyens par lesquels un dépôt, un `harness.toml` ou une sortie d'agent
  malveillants peuvent exécuter du code sur l'hôte qui exécute `harness`
- l'interface web locale (`harness ui`) : l'atteindre depuis une autre
  origine ou un autre hôte, contourner le jeton, ou une attaque CSRF contre
  ses points d'écriture
- un manifeste qui indique `Pass` pour une modification qui devrait être
  `Blocked`

Hors périmètre :

- les problèmes qui supposent un attaquant contrôlant déjà l'hôte ou le
  compte de l'utilisateur
- `allow_egress = true` ou d'autres réglages que l'utilisateur a
  explicitement assouplis (l'interface web demande une confirmation avant
  de les enregistrer)
- les vulnérabilités de Docker, Wasmtime ou d'autres dépendances
  elles-mêmes : signalez-les en amont ; un signalement montrant que harness
  les utilise de façon dangereuse est, lui, dans le périmètre
- l'absence de durcissement sans impact démontré, comme des en-têtes HTTP
  manquants sur l'interface web accessible uniquement en loopback

## Exonération (safe harbor)

Nous n'engagerons ni ne soutiendrons aucune action en justice contre une
recherche menée de bonne foi, qui respecte la présente politique, évite les
atteintes à la vie privée et les interruptions de service, n'utilise que
vos propres systèmes et dépôts de test, et nous laisse un délai raisonnable
pour corriger le problème avant divulgation.

## Versions prises en charge

Avant la 1.0, seule la dernière version publiée reçoit des correctifs ; ils
arrivent d'abord sur `main`. Après la 1.0, ce sera la dernière version
mineure de la version majeure en cours.

## Comment ce dépôt est protégé

- Chaque pull request exécute cargo-deny (licences et avis RustSec),
  gitleaks et la revue des dépendances de GitHub, et doit les réussir pour
  être fusionnée.
- L'analyse des secrets avec protection des push, les alertes Dependabot et
  les mises à jour de sécurité Dependabot sont activées.
- Les jetons des workflows sont en lecture seule par défaut, et les Actions
  ne peuvent pas approuver de pull requests.
- Le Rust `unsafe` est interdit dans tout le workspace.
