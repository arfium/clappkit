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

## The house documents

They live here because every clapp already carries this repo — one copy, no drift.

| | |
|---|---|
| [`docs/protocol.md`](docs/protocol.md) | **The Clapp Protocol.** Normative: manifest, wire, vocabulary. It wins over everything else. |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | The model — two channels, one state, two surfaces. |
| [`docs/TEMPLATE.md`](docs/TEMPLATE.md) | Forking a template into a new clapp. |
| [`docs/ICONS.md`](docs/ICONS.md) | The icon standard, and why the Dock needs its own. |
| [`docs/PLAYBOOK.md`](docs/PLAYBOOK.md) | Shipping rules learned by getting them wrong. |
