# clappkit docs

Two kinds of document: the **contract**, which Clatch enforces, and the **house standards**,
which are ours. When they disagree, the contract wins.

## The contract

| | |
|---|---|
| [`elements.md`](elements.md) | **The three types** — clapp:app, clapp:cli, skill: what each one is, what it ships, what it may and may not do. Start here. |
| [`format.md`](format.md) | **The `.clapp` format** — the depot layout, every `clatch.json` field, the type matrix, the picture limits, per-platform depots. Read at install. |
| [`protocol.md`](protocol.md) | **The control pipe** — how a running clapp:app and the launcher talk: transport, framing, vocabulary, signals, lifecycle, errors. |

**These three are the source of truth.** The Clatch launcher validates and installs
against them, and anything else that opens a `.clapp` reads it the same way. Where an
implementation disagrees, the implementation is the bug. Changes land here first.

## The house standards

| | |
|---|---|
| [`architecture.md`](architecture.md) | The model a clapp is built on — two channels, one state, two surfaces. |
| [`template.md`](template.md) | Forking the template into a new element. |
| [`icons.md`](icons.md) | The marks: the icon standard, the Dock's own inset, and what belongs in a banner. |
| [`playbook.md`](playbook.md) | Rules learned by getting them wrong. Read before shipping. |

## Where to start

- **Which type am I building?** [`elements.md`](elements.md)
- **What may my manifest say?** [`format.md`](format.md) — the fields, the limits, and what
  a launcher refuses.
- **Building a clapp:app?** [`architecture.md`](architecture.md), then
  [`template.md`](template.md).
- **About to ship?** [`playbook.md`](playbook.md).

## Not here

**The marketplace.** Listing an element, the publisher API, review and store pages belong
to clatch-server; its `docs/publishing.md` is the account of them. This repository defines
the package, not the shop — so if the two ever disagree about the shop, clatch-server is
right and this sentence is the only thing here that should have mentioned it.
