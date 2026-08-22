# Forking the template

Rename, replace the app, declare the surface, verify.

## 1. Rename — one command

Pick an **id** (reverse-DNS, `com.acme.notes`), a **CLI name** (short, lowercase, unique
among installed clapps, `notes`), and a display **name** (`Notes`):

```sh
scripts/rename.sh notes com.acme.notes "Notes"
```

It rewrites `clatch.json`, `package.json`, the Cargo manifests and the scripts, then
prints any leftover mentions. It does not touch your prose, your signals or your logic.

Identity is read from `clatch.json` at runtime, so there is no second copy in the code to
keep aligned — but three things must still agree, and only the last is checked for you:

| | |
|---|---|
| `connector.cli` | the `bin/<cli>` name the depot ships |
| `connector.signals` ids | the ids you actually emit |
| `connector.commands` | the verbs `<cli> -h` documents |

`clatch validate` reads the manifest; nothing reads your code. That gap is what
`npm run verify` exists to close.

## 2. Replace the app

Three files carry the demo; the rest is transport you keep.

- **`src-tauri/src/state.rs`** — your state and the methods that mutate it. Both surfaces
  call the same ones. Emit a signal **only on user actions**: the agent already knows about
  its own writes.
- **`src-tauri/src/cli.rs`** — one arm per verb, and the help text. That help is the
  agent's *only* manual; a verb missing from it does not exist as far as the agent knows.
- **`src/`** — the window. Talk to the core through `bridge.ts` and nothing else.

Leave `clappkit` alone: role dispatch, the bootstrap, both channels, paths, atomic writes
and the media boundary are shared and already correct.

## 3. Declare the surface

- **`connector.commands`** — one `{name, about}` per verb. Each becomes its own grant
  (`Bash(<cli> <name>:*)`), so a human can grant a subset.
- **`connector.signals`** — every signal, as `{id, type}` with `type ∈ run | context |
  buffered`. The declaration is the authority: Clatch re-validates the wire type against it
  and drops a mismatch. Declaring `poke` as `run` is what makes it wake an agent.
- **`launch`** — per-OS, with no cross-OS fallback. Ship `macos`, `windows` and `linux`
  entries only for the binaries you actually build.

## 4. Verify

```sh
npm run verify     # build → package → clatch validate → CLI ⇄ GUI round-trip
```

Then the real path, which is the one that catches packaging mistakes:

```sh
npm run pack
clatch install ./<id>-*.clapp && clatch run <id>
<cli> status
```

## Always-on apps

Clatch ships no scheduler and no app autostart — no clapp is started at boot. An app
that must act between user sessions keeps its own loop while running and emits a `run`
signal when it fires. Persistence and
missed-schedule policy are yours; Clatch gives you the wake and nothing more.
`clock-clapp` is the worked example.

## Gotchas

- `connector.cli` is **mandatory**. There is no CLI-less clapp; a manifest without one is
  rejected at validate and install.
- Command names must be non-empty and unique.
- A declared `icon` must exist on disk, or validate and install both fail.
- `bin/<cli>` is a dev wrapper in the repo and the compiled binary inside `pkg/`. On macOS
  the depot's copy lives in a `.app` bundle, and `package.sh` rewrites `launch` and
  `cliBin` to match — read those from the depot manifest, never assume `bin/<cli>`.
