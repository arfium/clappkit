# The clapp playbook

Rules learned by getting them wrong. Each one costs an afternoon if you rediscover it.

## 1. A `.clapp` ships real files only — no symlinks

The depot is unpacked on someone else's machine. A symlink into your source tree resolves
to nothing there. `scripts/package.sh` copies; never link.

## 2. Native first; a sidecar only when a library forces it

A pure-Rust clapp is a few MB. Vendoring a runtime makes it tens.

- **The service speaks HTTPS/JSON → do it in Rust.** Telegram's Bot API is plain HTTPS, so
  `telegram-clapp` has no sidecar. Mail is IMAP + SMTP, so neither does `mail-clapp`.
- **The protocol only exists as one library → bundle it.** WhatsApp Web is the Signal
  protocol over a websocket and only Baileys implements it, so `whatsapp-clapp` is a Rust
  app driving a bundled Node sidecar over stdio.

A sidecar must: speak newline-delimited JSON on stdin/stdout, keep stdout clean of logs
(silence the library's logger), and **die with its parent** — exit on stdin EOF.

## 3. Wear the app's brand, not the template's

A fork that still looks like the template reads as unfinished. Replace the tokens in
`src/styles.css`, the mark in `assets/`, and the window's copy. Each clapp is on-brand for
its own service; there is no shared theme.

## 4. Keep source and manifest in lockstep

`clatch validate` checks the manifest; nothing checks that your code matches it. A verb in
`connector.commands` that the CLI doesn't implement is a permission the agent is granted
and a command that fails. `<cli> -h` must list exactly the declared verbs.

## 5. `pkg/` and `*.clapp` are derived — never commit them

`scripts/package.sh` assembles `pkg/`; `clatch pack` produces the `.clapp`. Both are
gitignored. The committed sources are the truth; the depot is a copy you refresh.

## 6. Distribution is a tag, not a folder

```sh
clatch install github:<owner>/<repo>            # latest release
clatch install github:<owner>/<repo>@v0.1.0     # a tag
clatch install ./<id>-macos-arm64.clapp         # a downloaded depot
```

Push a `v*` tag and `release.yml` builds `<id>-macos-arm64.clapp` (+ `.sha256`) and
publishes it. A sidecar clapp also needs that toolchain on the runner.

## 7. Verify against real Clatch, not just the build

`npm run verify` proves the two surfaces talk. Before calling a clapp done, prove it
installs and runs:

```sh
clatch install ./<id>-*.clapp
clatch run <id>            # must reach a registered state
<cli> status               # round-trips against the installed app
```

For a sidecar clapp, confirm the app spawns the **depot's own** `vendor/node`
(`pgrep -fl <sidecar>`) and that `clatch stop` cleans it up.

## Field notes

- **`npm run build`, never bare `cargo build`.** Without Tauri's `custom-protocol` feature
  the binary loads the dev URL and you get a white window.
- **`cargo build` cannot see `#[cfg(test)]`.** Test code rots silently while everything
  looks green. Run `cargo test` before you believe anything.
- **rustls, never native-tls.** One binary that keeps cross-compiling.
- **A declared `icon` must exist on disk**, or validate and install both fail.
- **Baileys `405 Connection Failure`** is a stale WhatsApp-Web version or rate-limiting
  from hammering reconnects. Use `fetchLatestBaileysVersion()` and back off.
- **BSD `sed` has no `\b`.** Scripts that must run on stock macOS can't use GNU-only regex.
- **The shell cwd resets between tool calls** in an agent harness. Use absolute paths.
