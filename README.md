# clappkit

The shared foundation every clapp is built on: role dispatch (one binary, two roles), the
Clatch control pipe, the app's own GUI↔CLI socket, paths, atomic persistence, the
outbox/quarantine boundary for files, icons, and the Tauri glue.

```toml
clappkit = { path = "clappkit", features = ["tauri"] }
```

Each clapp carries this repo as a submodule, so a clone builds on its own:

```sh
git clone --recurse-submodules …    # or: git submodule update --init
```

## Documentation

Everything is in **[`docs/`](docs/)** — start at [`docs/index.md`](docs/index.md), which
says which document answers what.

They live here because every clapp already carries this repo — one copy, no drift.

| | |
|---|---|
| [`docs/format.md`](docs/format.md) | **The `.clapp` format** — the depot, every manifest field, the limits. |
| [`docs/protocol.md`](docs/protocol.md) | **The control pipe** — how a running app and the launcher talk. |
| [`docs/architecture.md`](docs/architecture.md) · [`docs/template.md`](docs/template.md) · [`docs/icons.md`](docs/icons.md) · [`docs/playbook.md`](docs/playbook.md) | The house standards. |
