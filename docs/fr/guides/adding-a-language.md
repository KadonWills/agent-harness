<!-- translated-from: docs/guides/adding-a-language.md@c6ab4eab17e4 -->
[English](../../guides/adding-a-language.md) | **Français**

> Traduction de la version anglaise, qui fait foi en cas de divergence.

# Ajouter un analyseur de langage

Objectif : un nouveau langage s'ajoute en **moins de 100 lignes** de code
d'intégration. Python tient en 29 lignes et JavaScript en 37
(`crates/harness-ast/src/lang/`). Si votre PR est nettement plus volumineuse,
c'est le framework qui vous fait défaut : ouvrez une issue.

## Ce que vous écrivez

Une constante `LanguageSpec` : une table de correspondance entre les types de
nœuds Tree-sitter de votre grammaire et les concepts que comprend le parcours
d'arbre partagé. Vous n'écrivez ni parcours d'arbre ni calcul de diff :
`crates/harness-ast/src/walker.rs` s'en charge pour tous les langages.

```rust
// crates/harness-ast/src/lang/go.rs
use crate::walker::{LanguageSpec, Rule};

pub static GO: LanguageSpec = LanguageSpec {
    name: "go",
    extensions: &["go"],
    grammar: || tree_sitter_go::LANGUAGE.into(),
    functions: &[
        Rule::new("function_declaration", "name"),
        Rule::new("method_declaration", "name"),
    ],
    types: &[Rule::new("type_spec", "name")],
    members: &[Rule::new("field_declaration", "name")],
    receiver: None,
    imports: &[("import_spec", "path")],
    identifiers: &["identifier", "field_identifier", "type_identifier"],
    signature_fields: &["parameters", "result"],
};
```

| Champ | Signification |
|---|---|
| `functions` | Déclarations qui deviennent `FunctionAdded/Removed/Changed/Renamed`. `Rule::with_value(&[..])` restreint aux nœuds dont le champ `value` est de l'un des types indiqués (par ex. `const f = () => {}`). |
| `types` | Classes, interfaces, structs, enums. Les symboles qu'ils contiennent sont qualifiés (`User.save`). |
| `members` | Champs et méthodes déclarés dans le corps d'un type. En supprimer un émet `MemberRemoved`. `Rule::with_name_kinds(&[..])` filtre selon le type du nœud de nom. |
| `receiver` | Règle facultative `self.x = …` / `this.x = …`, afin que les attributs affectés dans les méthodes comptent comme membres. |
| `imports` | Paires `(node kind, field)` dont le texte nomme un module. |
| `identifiers` | Types de nœuds dont le texte est un nom référencé. Ils alimentent la détection de conflits. |
| `signature_fields` | Champs de fonction dont la modification signifie `SignatureChanged`. |

Trouvez les types de nœuds et les noms de champs dans le fichier
`src/node-types.json` du crate de votre grammaire, ou avec
`tree-sitter parse file.ext`.

## Liste de contrôle pour une PR d'analyseur

- [ ] Dépendance de grammaire, optionnelle, dans `crates/harness-ast/Cargo.toml`,
      ainsi qu'une fonctionnalité `lang-<name>` (désactivée par défaut) ajoutée
      à `full`
- [ ] `src/lang/<name>.rs` contenant la spécification, enregistrée dans
      `src/lang/mod.rs`
- [ ] Paire de fixtures `tests/fixtures/<name>/before.<ext>` / `after.<ext>`,
      plus `expected.txt` (un `<Variant>:<symbol>` par ligne), et un test dans
      `tests/fixtures.rs`
- [ ] Entrée dans le tableau des langages pris en charge du README.md
- [ ] Commit conventionnel : `feat(ast): add <language> parser`

## Critères de revue

Les analyseurs sont jugés sur leurs fixtures. Les renommages, les changements
de signature, les membres supprimés et les ajouts de dépendances doivent
produire les bons événements `AstChange` sur du code réaliste, y compris du
code légèrement cassé (les agents en produisent). La récupération d'erreurs
de Tree-sitter en gère l'essentiel. La fixture TypeScript se termine
volontairement par une fonction non terminée.
