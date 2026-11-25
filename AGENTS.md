# engine

`hnl-engine` — foundation types for pluggable interpretation and rewrite
models: the shared substrate the language-operation engines build on —
the kernel-facing types and operation abstractions the `tool-*` /
`exec-*` families implement against.

## Layout

| path | role |
| --- | --- |
| `src/lib.rs` | crate root — engine abstractions |
| `src/{nbe,term_interpreter}.rs` | normalization-by-evaluation + term interpreter |
| `project/` | per-repo project docs worktree (ADR-0129) |
| `docs/` | engine docs |
| `xtask/` | vendored gate crate (`fmt`, `clippy`, `test`, `doc`, `gate`) |
| `workflows/gate.yaml` | per-push gate — `handl-gates` rust campaign via bupha |

## Contract

- Single-package Rust workspace; sibling deps are git dependencies
  (`branch = "main"`) — a plain clone test-runs as-is.
- `engine-eqsat` is the equality-saturation sibling; both share the
  `engine` substrate types.

## Verification

```bash
cargo xtask gate        # fmt --check → clippy -D warnings → test → doc
```

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->
