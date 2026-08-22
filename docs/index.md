# clappkit docs

Two kinds of document: the **contract**, which Clatch enforces, and the **house standards**,
which are ours. When they disagree, the contract wins.

## The contract

| | |
|---|---|
| [`format.md`](format.md) | **The `.clapp` format.** The depot layout, every `clatch.json` field, what each element type may declare, the picture limits, per-platform depots. Read at install. |
| [`protocol.md`](protocol.md) | **The control pipe.** How a running app and the launcher talk: transport, framing, the vocabulary, signals, lifecycle, errors. |

Both mirror the Clatch reference and name their sources at the top. Clatch is normative;
these copies exist so an element can read the contract offline.

## The house standards

| | |
|---|---|
| [`architecture.md`](architecture.md) | The model an element is built on — two channels, one state, two surfaces. |
| [`template.md`](template.md) | Forking the template into a new element. |
| [`icons.md`](icons.md) | The mark: the library standard, and why the Dock needs its own inset. |
| [`playbook.md`](playbook.md) | Rules learned by getting them wrong. Read before shipping. |

## Where to start

- **Publishing something?** [`format.md`](format.md) — the fields, the limits, and what a
  launcher refuses.
- **Building something?** [`architecture.md`](architecture.md), then
  [`template.md`](template.md).
- **About to ship?** [`playbook.md`](playbook.md).
