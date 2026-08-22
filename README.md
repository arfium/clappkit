# clappkit

The shared foundation for **clapps** — apps that run under the Clatch launcher, each one
binary serving two surfaces: a window for the human and a CLI for the agent.

This crate is the plumbing both surfaces need, so an app writes its own logic and nothing
else. Rust core, TypeScript front-end half, and the documents that define the contract.

## Using it

Each clapp carries this repository as a submodule, so a clone builds on its own:

```sh
git clone --recurse-submodules …     # or: git submodule update --init
```

```toml
# src-tauri/Cargo.toml
clappkit = { path = "../clappkit", features = ["tauri"] }
```

```ts
// src/bridge.ts — the front-end half, resolved by a Vite alias. No npm package.
export { cmd, onState, useSnapshot } from "@clappkit";
```

## What is in it

| Module | |
|---|---|
| `role` | One binary, two roles — which one this process is |
| `control` | The Clatch control pipe: bootstrap, register, agent roster, signals |
| `ipc` | The app's own GUI↔CLI channel, which Clatch never sees |
| `paths` · `store` | Where state lives; writing it atomically and privately |
| `media` | The outbox/quarantine boundary for files an agent sends or receives |
| `asset` · `window` · `snapshot` | Local images, window verbs, and the revision that orders the window's two writers |
| `icon` | The Dock/taskbar tile — *feature `icon`, on by default* |
| `app` | The Tauri glue — *feature `tauri`, off by default* |

The core is GUI-free and dependency-light: a headless clapp never compiles a PNG codec,
AppKit or a webview.

`web/` is the front-end half — the same two channels, the avatar cache and the snapshot
wiring, as plain TypeScript behind a Vite alias.

Where an OS difference is unavoidable it is written once and named: `ipc::address`,
`paths::user_base`, `store::atomic_write`.

## Documentation

Everything is in [`docs/`](docs/); [`docs/index.md`](docs/index.md) says which document
answers what. One copy, carried by every clapp.

| | |
|---|---|
| [`elements.md`](docs/elements.md) | **The three types** — clapp, cli, skill |
| [`format.md`](docs/format.md) | **The `.clapp` format** — the depot, every manifest field, the limits |
| [`protocol.md`](docs/protocol.md) | **The control pipe** — how a running app and the launcher talk |
| [`architecture.md`](docs/architecture.md) | The model: two channels, one state, two surfaces |
| [`template.md`](docs/template.md) | Forking the template into a new element |
| [`icons.md`](docs/icons.md) | The mark, and why the Dock needs its own inset |
| [`playbook.md`](docs/playbook.md) | Rules learned by getting them wrong |

The three contract documents mirror the Clatch reference and name their sources at the top.
Clatch is normative; these copies exist so an element can read the contract offline.

## Tests

```sh
cargo test --all-features
```

67 tests. Several set `CLATCH_DATA_DIR`, which is process-global, so they share one lock —
see `ENV_LOCK` in `src/lib.rs`.

## License

Apache-2.0. See [LICENSE](LICENSE).
