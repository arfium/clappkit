# The .clapp format

What a package contains and what its manifest may say: the depot layout, every field of
`clatch.json`, what each element type is allowed to declare, and the bounds a launcher
enforces before it will open one.

This is the **static** half of the contract, read at **install**. The runtime half — how a
running app and the launcher talk — is [`protocol.md`](protocol.md).

> **A mirror, not the source.** The normative text lives in the Clatch repository and wins
> wherever the two disagree: `reference/manifest.md` (the fields), `reference/elements.md`
> (the type matrix), `reference/launch.md` (the depot layout). Diff against those three and
> nothing else.

## The package

**A `.clapp` packages the two types that have a payload**: a zip rooted at `clatch.json`,
where the manifest's `type` inside selects the treatment — never the file extension, never
the repo name. **A skill ships as its `.md`** through the same routes; a document needs no
envelope.

```
com.example.clapp-macos-arm64.clapp
  clatch.json              the manifest, identical across this version's depots
  bin/<cli>                only the HOST platform's binaries (launch + cliBin)
  assets/icon.png          icon, banner, up to 4 photos
  <name>.clapp.sig         optional detached signature, checked before opening
```

**One platform per depot**, named `<id>-<os>-<arch>.clapp`. The launcher picks the host's
on install, widest match last — `<os>-<arch>`, then `<os>-any`, then a cross-platform
`<id>.clapp` for script-only apps — and no match is a loud error listing what the release
ships. Only the host launch command is validated.

**Nothing runs on install.** Opening one is file extraction only; the app executes later
through `clatch run`.
## The manifest — `clatch.json`

The app's static declaration, read at **install**. It is the single source for
everything Clatch knows about the app before it runs: identity, how to launch, and
the agent-facing surface.

```jsonc
{
  "manifestVersion": 1,                     // schema major
  "id": "com.example.clapp",                // reverse-DNS, path-segment safe
  "name": "Clapp",
  "description": "…",                       // library entry + context-inserted on grant
  "version": "0.1.0",
  "protocol": 2,                            // control-pipe major this app targets (§6, [`format.md`](format.md)2)
  "icon": "assets/icon.png",                // optional; banner/about/tags also optional
  "launch": { "macos": "bin/clapp", "args": ["app"] },   // ≥1 per-OS command
  "connector": {                            // agent-facing surface; every field optional
    "cli": "clapp",                         // the CLI shorthand; `<cli> -h` is the manual
    "cliBin": "bin/clapp",                  // optional; default bin/<cli>
    "commands": [ { "name": "set", "about": "…" } ],    // permission grain: Bash(<cli> <name>:*)
    "signals":  [ { "id": "poke", "type": "run" } ]     // declared vocabulary, typed (Signals)
  }
}
```

| field | required | rule |
|---|---|---|
| `manifestVersion` | yes | integer, `1` |
| `type` | no (default `clapp`) | `clapp` \| `cli`. **Never `skill`** — a skill is a Markdown file with front matter, not a package. The default keeps every pre-taxonomy manifest valid |
| `id` | yes | reverse-DNS, path-segment safe (no `/`, `..`) |
| `name` · `description` · `version` | yes | non-empty strings |
| `protocol` | yes | integer; the control-pipe major this app targets (Handshake) |
| `launch` | yes | ≥1 per-OS command (`macos`/`linux`/`windows`), optional `args` |
| `icon` · `banner` · `about` · `tags` | no | presentation (library page) |
| `publisher` | no | who published it. A package's reverse-DNS id implies its maker; a skill has no such id, so its ribbon is drawn from this and the name |
| `photos` | no | up to 4 screenshots, in the order shown; paths relative to the content root |
| `connector.cli` | **yes** | the CLI shorthand the agent types; `<cli> -h` is the whole manual |
| `connector.cliBin` | no | a NAME relative to the content root, resolved with host executable extensions; default `bin/<cli>` |
| `connector.commands` | no | `[{name, about}]` — the permission grain + library display; NOT the manual |
| `connector.signals` | no | `[{id, type}]`, `type ∈ run \| context \| buffered` — declared and typed (Signals) |
| `connector.login` · `loginCheck` · `logout` | no | **`cli` only** — the tool's own auth verbs |

**There is no CLI-less element.** `connector.cli` is the floor for every type: the CLI
is the constant surface an agent drives, so a manifest without one is rejected at
validate and install.

### What each type may declare

| | `clapp` | `cli` |
|---|---|---|
| `launch` | **required** | **forbidden** |
| `connector.cli` (+ `cliBin`) | required | required |
| `connector.signals` | optional | **forbidden** — a cli has no app→agent path |
| `connector.login` / `loginCheck` / `logout` | **forbidden** — the app's GUI owns auth | optional |

**Forbidden means rejected**, not ignored: a silent drop would let a package believe it
declared something it never got.

**A manifest may never say `skill`.** A skill has no `clatch.json` at all — it is a plain
`.md` whose YAML front matter IS its manifest, and its `name` is its identity. One
claiming the type is refused, with where its metadata belongs instead.

The **advertised platforms are the `launch` OS keys** (a per-OS command is the claim
"runs on that OS"); there is no separate `platforms` field, and a distribution ships one
depot per platform. **Additive-only within a `manifestVersion`** — new optional fields
only, never a new mandatory one; a launcher ignores fields it does not know. A breaking
change bumps `manifestVersion`.

### Presentation assets — `icon`, `banner` & `photos`

Optional, but when shipped they carry a fixed standard (as the agent avatar does),
checked at install for format and resolution. Aspect is a design target, not a hard
check — the GUI scales every asset with `cover`, so a mismatch crops, never letterboxes.

| | `icon` | `banner` | `photos` |
|---|---|---|---|
| role | app mark — library tiles + the detail hero (rendered 76px) + shortcuts | the library detail **hero** strip, behind the identity text | what the app LOOKS like: screenshots on the library page and a marketplace listing |
| count | 1 | 1 | **at most 4**, shown in the manifest's order |
| format | PNG (the desktop app icon) | PNG / JPEG / WebP | PNG / JPEG / WebP, by **magic bytes** — the extension follows the format, it does not declare it |
| aspect | **1:1** (square) | **215:32** (≈ 6.72:1) — design canvas `860×128` | free |
| min resolution | **512×512** | **3440×512** | — |
| max resolution | 1024×1024 | — | **1920×1080** |
| max file | 1 MiB | 2 MiB | **2 MB each** |

A photo that breaks a rule is **rejected at validate/install**, like any other manifest
error, so a package cannot ship art the launcher would have to refuse to draw later.
**Nothing is resized on the way in**: shipping a 4000px screenshot is telling the launcher
to draw something it never checked. Four is a shelf, not an album, and the 2 MB ceiling is
what keeps a depot downloadable on the connection somebody actually has.

The banner renders as a **128px-tall, ≤860px-wide** hero, `cover`-cropped and centered,
under a **left-dark horizontal scrim** (white identity text sits over the left ~40%).
So: keep focal imagery **center/right**; match the **6.72:1** ratio (the height is
fixed — an off-ratio image loses its top/bottom); and expect the sides to crop on a
narrow window. The `icon` is just the desktop app icon — no separate asset.
