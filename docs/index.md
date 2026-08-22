# clappkit docs

Two kinds of document: the **contract**, which Clatch enforces, and the **house standards**,
which are ours. When they disagree, the contract wins.

## The contract

| | |
|---|---|
| [`elements.md`](elements.md) | **The three types** — clapp, cli, skill: what each one is, what it ships, what it may and may not do. Start here. |
| [`format.md`](format.md) | **The `.clapp` format** — the depot layout, every `clatch.json` field, the type matrix, the picture limits, per-platform depots. Read at install. |
| [`protocol.md`](protocol.md) | **The control pipe** — how a running clapp and the launcher talk: transport, framing, vocabulary, signals, lifecycle, errors. |

All three mirror the Clatch reference and name their sources at the top. Clatch is
normative; these copies exist so an element can read the contract offline.

## The house standards

| | |
|---|---|
| [`architecture.md`](architecture.md) | The model a clapp is built on — two channels, one state, two surfaces. |
| [`template.md`](template.md) | Forking the template into a new element. |
| [`icons.md`](icons.md) | The mark: the library standard, and why the Dock needs its own inset. |
| [`playbook.md`](playbook.md) | Rules learned by getting them wrong. Read before shipping. |
| [`publishing.md`](publishing.md) | Getting an element into the Arfium marketplace. |

## Where to start

- **Which type am I building?** [`elements.md`](elements.md)
- **Publishing something?** [`format.md`](format.md) — the fields, the limits, and what a
  launcher refuses.
- **Building a clapp?** [`architecture.md`](architecture.md), then
  [`template.md`](template.md).
- **About to ship?** [`playbook.md`](playbook.md), then [`publishing.md`](publishing.md)
