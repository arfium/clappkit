# TAXONOMY

Every noun this contract uses, defined once. Other documents use these words and do not
redefine them; where one must be pinned harder, its own section is cited.

## 1. The unit

| term | is |
|---|---|
| **element** | one thing carried under one identity and one library card: a package — `clapp:app` or `clapp:cli` — or a `skill`. This directory owns what the kinds are and what each declares; installing, listing and distributing them is the launcher's and the registry's ([index.md](index.md) § 1. Ownership of layers) |
| **kind** | which of the three an element is: `clapp:app`, `clapp:cli`, `skill`. The word for the concept, everywhere it is discussed, listed or compared |
| **type** | the **manifest field** that carries a package's kind in short form: `"clapp"` or `"cli"`. A skill has no manifest, so the field never appears in one ([elements.md](elements.md) § 2. The name and the field) |
| **id** | the machine identity, reverse-DNS and path-segment safe: `com.arfium.clock` |
| **version** | one release of an element: `0.3.0`; semver, never replaced once published |

## 2. The package

| term | is |
|---|---|
| **package** | a `.clapp` file: a zip rooted at `clatch.json`, carrying the bytes for **one** target |
| **depot** | the extracted package — the tree the launcher installs and runs from |
| **content root** | the depot's top directory, where `clatch.json` sits; every manifest path is relative to it |
| **target** | which machine a package is for: `macos-arm64`, `windows-x64`, `any` |
| **host pair** | the running machine's `<os>-<arch>`, which selects the depot it may install |
| **safe segment** | a path component of `[A-Za-z0-9._-]` with no `..`, no `*`, no whitespace — the rule `id` and every manifest path obey, because the value is interpolated into an exec shim |
| **skill** | the one element that is not a package: a single Markdown file whose YAML front matter is its whole manifest, named not versioned |

## 3. The manifest

`clatch.json` — the app's static declaration, read at install. Every field is defined in
[format.md](format.md) § 3–8; the terms are named here.

| term | is |
|---|---|
| **manifest** | `clatch.json`: identity, how to launch, and the agent-facing surface |
| **manifestVersion** | the manifest schema's major; a breaking schema change bumps it ([format.md](format.md) § 2. Versioning) |
| **name** | the display name a person reads; it may change, the `id` may not |
| **connector** | the manifest's agent-facing block: the CLI, its commands, its signals, its login verbs |
| **cli** | the shorthand an agent types — a **name**, resolved like an executable, never a filename |
| **cliBin** | the path to the binary, a safe segment tree; default `bin/<cli>` |
| **command** | one separately grantable verb; the list is the permission grain, not the manual |
| **launch** | how a clapp:app's process is started; **forbidden** on a clapp:cli |

## 4. The control pipe

The runtime channel a clapp:app and the launcher talk over; the whole of it is
[protocol.md](protocol.md).

| term | is |
|---|---|
| **control pipe** | the local, per-app duplex channel Clatch and a running clapp:app exchange messages on |
| **protocol** | the control pipe's major version, declared by a clapp:app in its manifest and matched by the launcher ([protocol.md](protocol.md) § 11. Security and versioning) |
| **envelope** | the JSON-RPC 2.0 message the pipe carries |
| **frame** | one length-delimited envelope on the wire |
| **register** | the first thing a clapp:app does on the pipe: announce itself and its protocol |
| **signal** | a typed notice a clapp:app sends toward its agent; declared in the manifest, and the declaration is the authority |
| **signal type** | `run`, `context` or `buffered` — see § 7 |

## 5. The surfaces and the agent

| term | is |
|---|---|
| **window** · **GUI surface** | the human's face of a clapp:app; owns its own auth |
| **CLI surface** | the agent's face — the `cli` and its granted commands; the one surface every element has |
| **state** | the single source of truth a clapp:app's two surfaces both read and drive |
| **agent** | the automation Clatch binds to an element; reaches a clapp only through its CLI |
| **grant** | the permission that lets an agent run a command; **installing grants nothing** ([elements.md](elements.md) § 3. Three rules) |

## 6. Pictures

Defined once here; the limits are [format.md](format.md) § 9. Pictures. The craft is [icons.md](icons.md).

| term | is |
|---|---|
| **icon** | the app's square mark, 1:1, the library tile and detail hero |
| **banner** | the one wide strip behind the detail identity text, 215:32 |
| **photo** | one of up to four screenshots of what the app looks like |

## 7. Closed sets

| set | values | home |
|---|---|---|
| kind | `clapp:app` · `clapp:cli` · `skill` | [elements.md](elements.md) § 1. The three kinds |
| manifest `type` | `clapp` · `cli` (a skill has none) | [format.md](format.md) § 8. What each kind may declare |
| signal `type` | `run` · `context` · `buffered` | [protocol.md](protocol.md) § 7. Signals |
| picture role | `icon` · `banner` · `photos` | [format.md](format.md) § 9. Pictures |
| `target` | `<os>-<arch>` per the host pairs, plus `any` | [format.md](format.md) § 10. Distribution |

`run` starts a turn on an idle agent and queues on a busy one; `context` is queued for the
agent's next turn; `buffered` rides the user's next prompt and never stacks.

## 8. Terms that collide

| this | is not this |
|---|---|
| **element** | **clapp** — an element is the catalog unit; a clapp is one kind of element; a `.clapp` is the file one ships in |
| **kind** (`clapp:app`) | the manifest field `type` (`clapp`) — the kind names the thing; the field carries only its short form |
| `name` (the display name) | `id` (reverse-DNS), and both differ from `cli` (the agent's shorthand) |
| `manifestVersion` (schema major) | `protocol` (control-pipe major) — two independent versions in one manifest |
| `cli` (a name) | `cliBin` (a path) |
| `command` (a grantable CLI verb) | a protocol **verb** (a pipe method, [protocol.md](protocol.md) § 6. Vocabulary) |
| `launch` (how an app starts) | `run` (a signal type), and both differ from `clatch run` (the launcher verb a clapp:cli refuses) |
| a signal's `id` (its stable name) | a per-emission value — a signal is declared once and sent many times |
| **grant** (permission to run a command) | **install** (having the element at all); visibility and permission are separate |
