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

Take the brand from the live site, not from memory or a screenshot. The colours are
usually sitting in CSS custom properties, and the mark is often a glyph in the site's own
icon font — `jlcpcb-clapp` pulled its wordmark out of one at U+E6CC and traced it to a
path, which is why it is the real mark and not a lookalike. A lookalike logo is the one
detail every user notices.

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

Push a `v*` tag and `release.yml` builds `<id>-<os>-<arch>.clapp` (+ `.sha256`) and
publishes it. A `.clapp` is per-OS-arch, so a release is **one depot per platform**, each
carrying its own rewritten `connector.cliBin` — `bin/<cli>.exe` on Windows, a path into
the `.app` on macOS. A sidecar clapp also needs that toolchain on the runner.

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

## 8. A dependency you cannot reach is not a dependency

A private git dependency turns every clone, every CI run and every contributor into a
credentials problem: a deploy key, a PAT in a secret, a submodule token, and a release that
cannot be re-cut when the token expires. Vendor it instead.

```toml
# src-tauri/Cargo.toml — the crates.io/git coordinates stay; only the source moves.
[patch."ssh://git@github.com/arfium/clatch.git"]
clatch-core = { path = "../vendor/clatch/crates/clatch-core" }
```

Copy the crates in at a **pinned tag**, keep a `scripts/vendor-clatch.sh` that refreshes
the copy at a newer one, and record which tag is in there. The standing proof that nothing
private is left is a build with no network and no lockfile drift:

```sh
cargo build --offline --locked
```

Then the workflow needs no secret at all. `secrets.*` is also not usable in a job-level
`if:` — if you find yourself writing a `gate` job to work around that, you are still
solving the wrong problem.

## 9. Windows is the platform that finds your assumptions

Every one of these failed a real release run, in this order:

- **`icons/icon.ico` is mandatory.** `tauri-build` compiles a Windows resource from the
  first `.ico` in `bundle.icon` and fails the build without one — even with
  `bundle.active: false`. Generate it from the SVG at 16/24/32/48/64/128/256 and commit it;
  Windows picks a different size per context and a scaled-down 256 looks it.
- **`npm run <script>` spawns cmd.exe**, which cannot run a `.sh` file. Call
  `bash scripts/package.sh` directly in CI, or `npm config set script-shell bash` first.
- **`.gitattributes` with `* text=auto eol=lf`.** Checkout with CRLF and every bash script
  dies on `\r`. Add binary rules for icons and fonts in the same file.
- **Git Bash has no `zip`.** It has `7z`. Both produce the zip that a `.clapp` is —
  `zip -r` with a `7z a -tzip` fallback.
- **WebView2: loader vs runtime.** The MSVC toolchain links the *loader* statically, so the
  depot ships zero DLLs — assert the host is `*-msvc` in CI, because a `-gnu` host would
  need `WebView2Loader.dll` beside the exe. The *runtime* is a separate machine-wide
  Microsoft install and no toolchain gives you that: check for it before the window opens
  and print the download link instead of dying silently
  (`src-tauri/src/webview.rs`, `#[cfg(windows)]`, called from `main_dispatch`'s GUI arm).
- **A cold Windows build is ~15 minutes** against 45 seconds on macOS. Two causes, both
  fixable: `Swatinem/rust-cache` does not save on a failed run, so a build that dies never
  seeds a cache and the next attempt starts from zero — set `cache-on-failure: true`. And
  Defender scans all ~100k files rustc writes; excluding the workspace, `~/.cargo` and
  `~/.rustup` is worth roughly a third of the time on a throwaway VM.

The cheapest missing-DLL check is not reading the import table — it is running the binary.
Windows resolves a PE's imports at process start, so an exe with an unsatisfied dependency
cannot print `--help` at all. The CLI role is console-subsystem and needs no display, which
is what makes it the thing a headless runner can smoke-test:

```sh
"$bin" --help | head -n 1
"$bin" status   # must FAIL, with the app's own "not running" sentence
```

That second line is the better half. It proves the CLI role got as far as dialling its
socket, so the two-surface wiring survived packaging.

## 10. Probe the vendor's API before you write the parser

Documentation describes the API the vendor meant to ship. Send real requests and read real
bytes; the difference is where the afternoon goes.

- **Absent is not empty.** JLCPCB returns `list: null`, not `[]`, for a search that matches
  nothing. Parsed strictly that reads as a catalog failure and the app tells the user the
  service is down. Treat a missing list as zero rows; error only when the page object
  itself is missing.
- **A parameter the server ignores is worse than one it rejects.** JLCPCB accepts
  `sortField`/`sortType`/`orderBy` on both API versions and honours none of them. Sorting
  the page you happen to hold produces pages that each look sorted and are collectively
  nonsense. If the server will not sort, sort a **pool**: collect N pages in chunks, sort
  the pool, page the pool — and label it honestly ("across all 312" vs "first 400 of
  12,904") so nobody mistakes a bounded sort for a global one.
- **A quota is an architecture, not a counter.** Digi-Key allows 1000 calls/day. That
  budget survives only with all four of: a request `Gate` (mutex + minimum gap between
  calls), TTL caches per response kind, single-flight coalescing so N identical in-flight
  requests become one, and **no background polling of any kind**. Then show what is left,
  in the window and in the CLI answer — an invisible quota is one the user burns.

## 11. Page the shared state, not the caller's request

Page size belongs to the state both surfaces see. The moment a caller's `-n` sets it, an
agent asking for 1 result silently repaginates the human's window to one row per page —
the bug reads as "why does searching stm32 return one part?". `-n` limits what the terminal
*prints*; the page is fixed and both surfaces say "N of TOTAL" about the same page.

Same rule for sort: it is state, so it re-pages. A control that only reorders the current
page is a lie about the data underneath it.

## 12. Scripts read the manifest; they never guess paths

`verify.sh` looked for `bin/<cli>` and broke the day macOS packaging moved the binary into
the `.app` bundle. The depot's manifest already carries the answer — `install_manifest`
wrote it there — so read `connector.cliBin` out of `pkg/clatch.json` and use that. Any
script that hardcodes a layout will be wrong on one of the three platforms.

## 13. Credentials live in the app's data dir, and nowhere else

A clapp that needs an API key takes it in **the window**, from the human. Never ask for it
in chat, never accept it through a CLI verb, and give the agent no verb that can read it
back. It is written to the app's private data dir and it never appears in a snapshot, a log
line or a CLI answer — snapshots are the one structure that goes everywhere, so anything
secret must be absent from them by construction, not by redaction. What both surfaces show
is whether it is *connected*, which is all either of them needs to know.

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
- **`src/preview.ts` renders the UI in a plain browser** against a fake snapshot. It is how
  you look at the window without a build, and how you check the two states an agent-driven
  screen has (tinted, mid-flight) without waiting on the vendor's API.
- **Sum an empty `f64` list and you get `-0.0`**, so an empty BOM prints `$-0.00`. Add
  `+ 0.0` when rounding to cents, and pin it with a test — nobody finds this twice.
- **No "ask the agent" button.** A clapp's window is for the human's own actions; the agent
  arrives through Clatch. A button that prompts on the user's behalf inverts that.
- **Only human actions signal.** An agent is never told about its own write — that is the
  loop that makes an app talk to itself.
