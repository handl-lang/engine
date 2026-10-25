# How-to guides

Task recipes. Each assumes sibling checkouts of this repo and of the platform
repos it patches.

## Add a new engine

Implement `Model` for your engine type and let `interpret` drive it; the `engine-*` crates are the worked examples — no per-kind Rust branching.

```
struct MyModel;
impl Model for MyModel {
    // resolve the scheme's term for a node kind;
    // `interpret` then executes it generically.
}
```

## Inspect an AST without a cycle

Take `AstRef` + `LanguageView` instead of `handl_language::Ast`; the view resolves child positions by named role (ADR-0081).
