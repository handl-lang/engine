# engine

`hnl-engine` — foundation types for pluggable interpretation and rewrite models.

The `engine`/`engine-eqsat` layer: the shared substrate the language-operation
engines build on — the kernel-facing types and operation abstractions the
`tool-*`/`exec-*` families implement against.

## Development

```bash
cargo xtask gate        # fmt --check → clippy -D warnings → test → doc
```

`cargo xtask` wraps the repo's `xtask/` crate (`fmt`, `clippy`, `test`, `doc`,
`check`, `gate`); `ci.sh` is the gate entry. Deps resolve from git, so no
sibling checkouts are needed; a plain clone test-runs as-is.

## License

MIT.

## Documentation

<https://handl-lang.github.io/docs/engine/>
