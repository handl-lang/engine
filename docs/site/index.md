# engine

Foundation types for every pluggable engine: `Term`, `Model`, and the arena view.

## What it is

The substrate every other `engine-*` crate builds on. It defines the **`Term`**
algebra used to express eval schemes, the **`Model`** trait an engine implements,
and a minimal, object-safe **arena view** (`AstRef` / `AstNode` / `LanguageView`)
that lets a model inspect an AST without depending on `handl-language` — which
would be a dependency cycle, since `handl-language` already depends on
`hnl-engine` for `Term`.

## Modules
- `term` — `Term`, `ArrSource`, `Side`, `RewriteWitness`, handler elimination
- `term_data` — `TermData` — the owned, inspectable form of a term
- `term_interpreter` — `interpret` — the generic scheme interpreter
- `nbe` — normalization-by-evaluation helpers shared by the typecheckers


## Start here

- **Reference** — [public API](/engine/reference/)
- **How-to** — [task recipes](/engine/how-to/)
- **Explanation** — [design and rationale](/engine/explanation/)

No tutorials: this repo is a building block used from Rust. End-to-end
walkthroughs live with the product that composes it — see the
[Handl platform](/handl/).
