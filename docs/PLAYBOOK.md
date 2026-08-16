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

## 14. Continuous state reports on a threshold, not a clock

A map's camera changes sixty times a second under a dragging hand; an agent wants none of
those and must still eventually hear the drift. The pattern from `maps-clapp`: the window
reports only a **settled** value (debounced ~250 ms), and the core decides whether the
move is *news* — half a screen-width, or two zoom levels, judged against the last value
the agent was actually told, so nudges accumulate into one honest report. The same
threshold gates any per-move spending (a reverse lookup naming the view). Never a timer:
polling a continuous value is how you pay for silence.

## 15. Ambiguity is a state, not a guess and not an error

"Route to Taksim" names a square and a metro station. Picking one silently routes with
total confidence to the wrong place; refusing teaches nothing. `maps-clapp`'s answer:
park a **visible placeholder** in the shared state ("stop 3 — choosing"), put the
candidates in the result list both surfaces already render, and let either surface answer
— `select 2`, or a click on the row. The placeholders themselves are the queue for many
open questions at once; there is no second list to drift. Corollary: a clear winner is not
ambiguity — gate on the *margin* between the top two scores, and let an exact name match
be decisive.

## 16. The data already on screen answers first

Vector tiles are not pictures: OpenMapTiles carries a classified `poi` layer, decoded and
in memory for whatever the map is drawing. `maps-clapp` answers a category tap ("fuel")
from `querySourceFeatures` in the same frame, then lets the real (gated, remote) query
replace it a moment later — and the head start goes **through the shared state**, so the
agent sees what the human sees. A head start only one surface knows about is drift with
better manners. When the tiles carry nothing, the seed is silently inert: below a minimum
it does not fire at all.

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
- **"Insert before the first symbol layer" is folklore, not a rule.** OpenFreeMap's dark
  style opens with `water_name` at index 8 — before any road — so the advice buried a
  route under the whole city; liberty put bridges over it. Anchor overlays at the first
  symbol AFTER the last geometry layer: above every road, below every name.
- **`map.on("load")` may never fire**; install sources/layers on `styledata`, guarded by
  asking the map (`getSource(...)`) rather than a flag — then a deliberate theme
  `setStyle` rebuilds them for free.
- **Animated camera moves do not run while the document is hidden** (rAF stops): an
  agent's `goto` into a minimised window is silently swallowed. If nothing will animate,
  jump.
- **A packaging fallback that "just copies" ships the wrong artifact on exactly the
  machines nobody watches.** The Dock-icon inset step fell back to the raw full-bleed PNG
  when its tool was missing — correct on the author's machine, towering in the Dock on
  keyless ones. Run repo tools through the app's own manifest (so vendored patches apply)
  and fail loud.
- **A vocabulary one surface renders is core state, not UI.** maps-clapp's category chips
  lived in the window's code; the first agent on the CLI had to learn the word
  "restaurants" from a screenshot. Any enum a surface shows — chips, modes, filter names —
  belongs in the core and rides the snapshot, and a test should pin that the CLI's manual
  names every entry. The report that found it: "gui'de tag enumu var ise cli de bu enumu
  sunmalı ve birebir aynı çalışmalı."
- **Case-fold what the map's editors type**: the first live supermarket wrote its hours
  `mo-su 09:00-21:00`. Parsers meet data, not specs.
- **`src/preview.ts` renders the UI in a plain browser** against a fake snapshot. It is how
  you look at the window without a build, and how you check the two states an agent-driven
  screen has (tinted, mid-flight) without waiting on the vendor's API.
- **Sum an empty `f64` list and you get `-0.0`**, so an empty BOM prints `$-0.00`. Add
  `+ 0.0` when rounding to cents, and pin it with a test — nobody finds this twice.
- **No "ask the agent" button.** A clapp's window is for the human's own actions; the agent
  arrives through Clatch. A button that prompts on the user's behalf inverts that.
- **Only human actions signal.** An agent is never told about its own write — that is the
  loop that makes an app talk to itself.
