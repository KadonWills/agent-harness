<!-- translated-from: SUPPORT.md@53f53d0b8b64 -->
[English](../../SUPPORT.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Obtenir de l'aide

| Vous avez… | Où aller |
|---|---|
| Une question sur l'utilisation ou la configuration de harness | [Discussions → Q&A](https://github.com/KadonWills/agent-harness/discussions/categories/q-a) |
| Une idée, ou une modification à soumettre à la discussion avant une RFC | [Discussions → Ideas](https://github.com/KadonWills/agent-harness/discussions/categories/ideas) |
| Quelque chose qui ne fonctionne pas | [Un rapport de bug](https://github.com/KadonWills/agent-harness/issues/new?template=bug_report.yml) |
| Une vulnérabilité de sécurité | [Signalement privé de vulnérabilité](https://github.com/KadonWills/agent-harness/security/advisories/new) — jamais une issue publique |
| Un problème lié au Code de conduite | **Report content → Report to repository admins** sur le commentaire ou le fil concerné ; voir le [Code de conduite](CODE_OF_CONDUCT.md#comment-signaler) |
| Quelque chose que vous avez construit avec harness | [Discussions → Show and tell](https://github.com/KadonWills/agent-harness/discussions/categories/show-and-tell) |

Avant de poser une question, deux commandes répondent à la plupart d'entre elles :

```bash
harness check          # ce qui ne va pas dans la configuration ou le bac à sable, et le correctif
harness check --json   # la même chose, à coller dans un rapport de bug ou une discussion
```

Chaque erreur affichée par harness se termine par une ligne `fix:`. Si ce
n'est pas le cas, ou si le correctif ne fonctionne pas, c'est un bug — merci
de le signaler.

Ce projet pré-alpha est maintenu par des bénévoles. Il n'existe pas de support
payant et les délais de réponse ne sont pas garantis ; consultez
[MAINTAINING](../MAINTAINING.md#response-times) (en anglais) pour connaître
nos objectifs.
