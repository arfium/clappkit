# THE FORMAT CONTRACT

## 1. Ownership of layers

clappkit is the source both siblings align to: what a package *is* is decided here, and
`clatch/docs/` and `clatch-server/docs/` cite this directory rather than restate it.
Knowing whose rule a sentence is, is the difference between a specification and a copy that
drifts.

| layer | normative home | decides |
|---|---|---|
| **package** | **this directory** | what a `.clapp` or a skill `.md` *is* - manifest fields, the kind matrix, picture limits |
| **control wire** | **this directory** ([protocol.md](protocol.md)) | the bytes on the Clatch↔clapp pipe, vendored into every clapp |
| **distribution** | the publisher's GitHub release | which bytes exist, per platform, named `-<os>-<arch>.clapp` |
| **realm** | `clatch-server/docs/` | accounts, organizations, the catalog, review, who may see what |
| **this machine** | `clatch/docs/` | the launcher: install, lifecycle, agents, signals, the GUI, delivery |
| **turn execution** | `agent-engine/` | how one agent turn runs |

The boundary runs both ways: **this directory restates no foreign rule** - where a package
touches the realm or the launcher it names the owner and says the owner wins - and **no
other repository restates a package rule**. Where the launcher or the registry enforces a
format rule it cites this directory and says clappkit wins; a guide elsewhere that
describes a `.clapp` mirrors this directory and says so.

Two things this directory and the launcher both name - the injected `CLATCH_*` variables
and the control pipe - are kept in step by each naming the other, never by one restating
the other: `clatch/docs/` is the launcher's account, this directory is the app's.

## 2. Documents

| | is | defers to |
|---|---|---|
| [taxonomy.md](taxonomy.md) | every noun this contract uses, defined once, and the sets they are drawn from | - |
| [elements.md](elements.md) | the three kinds - what each one is, ships, and may declare | taxonomy.md for the terms |
| [format.md](format.md) | the `.clapp` package and every `clatch.json` field: layout, the kind matrix, the limits, the per-platform depot | taxonomy.md; elements.md for what a kind is |
| [protocol.md](protocol.md) | the control pipe a running clapp and the launcher talk over: transport, framing, vocabulary, signals, lifecycle | taxonomy.md; format.md for `protocol` and `connector` |
| [architecture.md](architecture.md) | the model a clapp is built on - one core, two surfaces, one state | protocol.md for the pipe |
| [template.md](template.md) | forking the template into a new element | elements.md; format.md |
| [icons.md](icons.md) | the marks - the icon standard, the Dock's inset, the banner | format.md § 9. Pictures for the limits |
| [playbook.md](playbook.md) | the rules learned by getting them wrong; read before shipping | every document above |

Read taxonomy.md first. A document does not restate its own purpose at its head - this
table is the only place it is written.

## 3. Reading order

| you are | read, in order |
|---|---|
| meeting a term for the first time | taxonomy.md |
| choosing which kind to build | taxonomy.md · elements.md |
| writing a manifest | taxonomy.md · format.md |
| building a clapp's window and CLI | architecture.md · protocol.md |
| forking the template | template.md · elements.md |
| drawing the icon and banner | icons.md · format.md § 9. Pictures |
| the launcher, or anything that opens a `.clapp` | format.md · protocol.md, then [`clatch/docs/`](../../../clatch/docs/index.md) for how it is installed and run |
| listing an element in the marketplace | [`clatch-server/docs/`](../../../clatch-server/docs/index.md) - the registry owns it |
| about to ship | playbook.md |

The two sibling contracts are [`clatch/docs/`](../../../clatch/docs/index.md) (the
launcher) and [`clatch-server/docs/`](../../../clatch-server/docs/index.md) (the registry);
they align to this one.

## 4. Format

The aim is that a reader finds an answer fast, not that every file looks the same.

| | |
|---|---|
| **Opening** | A title, then the first section. What a document is for is written once, in § 2 above |
| **Facts** | A table when entries share columns; a list when they do not. A field, a limit, a kind rule, a signal: table |
| **Interfaces** | An interface - a manifest object, a method, a message, a closed set - is ONE complete table: every field or value in it, and no prose between the rows. A reader implements from the table alone |
| **Reasoning** | Kept where it earns its place - a rule someone will undo unless they know why keeps its why. It follows the table; it never interrupts it |
| **Numbering** | Sections are numbered for citation, and stable: a section is appended, never inserted, so `§ 6` means the same thing next month |
| **Terms** | Defined in [taxonomy.md](taxonomy.md), used everywhere else |
| **References** | By document, section number and name: `format.md § 9. Pictures`. Both, so a renumber and a rename are each caught |

## 5. Enforcement

- **The contract is what consumers read.** The Clatch launcher validates and installs
  against these documents; anything else that opens a `.clapp` reads it the same way. Where
  an implementation disagrees, the implementation is the bug, and the change lands here
  first.
- **Additive by default.** New fields and signals appear; existing names and meanings do
  not quietly change. A change of meaning is written at the point of use, loudly, and moves
  the `manifestVersion` or `protocol` major it belongs to (format.md § 2. Versioning,
  protocol.md § 11. Security and versioning).
- **The gate runs on every push.** `scripts/check-docs.sh` refuses the drift a reader would
  otherwise find: a broken link, a document missing from § 2, a term defined twice, a
  manifest field format.md declares that no document names. It checks shape, never whether a
  written rule is true - that it leaves to a reader.
