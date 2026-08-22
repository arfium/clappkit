# Elements: clapp:app, clapp:cli, skill

An **element** is the unit Clatch installs, lists and distributes: one identity, one card
in the library. There are exactly **three types**, declared and never inferred. Two are
packages — a folder with a `clatch.json`. The third is a document.

| | **clapp:app** | **clapp:cli** | **skill** |
|---|---|---|---|
| What it is | a full Clatch app | a packaged command-line tool | one Markdown file an agent reads |
| Ships as | `.clapp` package | `.clapp` package | the `.md` itself |
| Manifest | `clatch.json` | `clatch.json` | its YAML front matter |
| Manifest `type` | `"clapp"` | `"cli"` | **never appears** |
| Process | `launch` | never | never |
| Control pipe | registers | never | never |
| Signals to the agent | declared, typed | **never** | never |
| Agent surface | its CLI, granted | its CLI, granted | none — knowledge, not commands |
| Login verbs | never — its window owns auth | optional | never |

### The name and the field

`clapp:app` and `clapp:cli` are the **element names** — what these things are called
everywhere they are discussed, listed or compared. `clapp:` is the namespace, and the part
after the colon is the kind within it.

Inside a `clatch.json` you are **already in that namespace**, so the `type` field carries
the short form and nothing else: `"clapp"` for a clapp:app, `"cli"` for a clapp:cli. A
manifest that writes the qualified name is not installable — the launcher matches the
short form exactly.

`skill` names both the type and nothing in any manifest: a skill has no `clatch.json` at
all, so the string never appears in one.

### Three rules

**The type is a contract, not a hint.** Each type has required *and forbidden* surfaces,
and validate/install reject anything that crosses them. There are no hybrids: a clapp:cli
that grows a window becomes a clapp:app by changing its type, not by drifting into one.

**A clapp:cli carries no path back to the agent.** No pipe, no register, no signals. An
agent reaches one the way a person does — by running it. That is the definition of the
type, not a missing feature.

**Installing grants nothing.** A grant does. Visibility and permission are separate.

The field-by-field rules are in [`format.md`](format.md); the runtime half a clapp speaks
is [`protocol.md`](protocol.md).

> **This is the source of truth.** Two implementations follow it — the Clatch launcher,
> which validates and installs, and the Arfium marketplace, which lists and serves. Where
> an implementation disagrees with this document, the implementation is the bug. Changes
> land here first.

## clapp:app

A full app: a window for the human, a CLI for the agent, one binary serving both over one
state. It declares `launch`, registers on the control pipe, and may emit typed signals
that wake or inform its agent.

Auth is its own window's business — the login verbs are forbidden here, because an app
with a screen has somewhere better to ask.

## clapp:cli

One well-formed command-line tool, packaged and formatted for Clatch. Content is
`clatch.json` + `bin/<cli>` + optional assets, shipped per platform exactly like a
clapp:app.

**`connector.cli` is a name, not a filename.** It resolves the way an OS resolves an
executable: the declared path, else that path plus the host's executable extension — so
`cli: "parts"` finds `bin/parts` on unix and `bin/parts.exe` on Windows from one manifest. Validate and
install share the resolver, so they cannot disagree.

There is **no lifecycle**: no instance, no run state, no focus. `clatch run` refuses, and
says the type is why.

### Login

Some tools are useless until signed in, and sign-in belongs to the vendor's own browser
flow. A cli may declare any subset of three verbs — the shape a tool cannot honour is the
shape it must not claim.

```jsonc
"connector": {
  "cli": "acme",
  "login":      "auth login",
  "loginCheck": "auth status",
  "logout":     "auth logout"
}
```

- `login` runs the verb, bounded. A tool that insists on a TTY fails fast, naming the
  command to run by hand.
- `loginCheck` is the **only** source of truth: exit 0 means signed in. Without it the
  state is unknown and never claimed.
- `logout` matters as much as `login`. A tool that can take a credential must give it
  back, and purge is a different act: it erases Clatch's copy while the vendor may still
  hold a live session only its own verb can end. Make it idempotent.

## skill

Knowledge, not a program: **one Markdown file** whose YAML front matter is its entire
metadata. No `clatch.json`, no folder, no package — a document needs no envelope.

```markdown
---
name: writing-style        # required, and the identity on this machine
description: How we write. # required, the line an agent reads to decide fit
publisher: arfium          # optional
tags: [writing, docs]      # optional
---
```

- **The name is the identity.** A document is named, not versioned into a package, so
  installing over a name that is taken replaces it.
- **No front matter is a refusal.** A description the launcher invented would be a skill
  nobody wrote.
- **A manifest may never say `skill`.** One that claims the type is rejected, and told
  where its metadata belongs instead — a skill has no `clatch.json` to claim it in.

## Naming

**An element's repository ends in `-clapp` whatever its type** — a clapp:cli lives in
`acme/parts-clapp`, never `acme/parts-cli`. The suffix names the ecosystem, not the kind,
so a repository never needs renaming when its type changes. The launcher reads the manifest and ignores the directory
name entirely.
