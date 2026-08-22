# Architecture

How a clapp is put together, and why. The contract itself — manifest, wire, vocabulary —
is [`protocol.md`](protocol.md); that document is normative and this one explains it.
Read that one when you need the exact shape of a field.

## The boundary

Clatch owns the launcher, the registry, the Clatch↔app control pipe, and the agent host.
It is **blind to your app's insides**. The agent operates your app by running *your own
CLI* in a shell; Clatch is never in that path.

So the contract is three surfaces:

1. **Manifest** (`clatch.json`) — how Clatch installs, launches and describes you.
2. **Control pipe** — how you register and signal the agent.
3. **CLI** — the agent's hands. Mandatory, and `<cli> -h` is its manual.

Everything else here is one good shape, not a requirement.

## One backend, two frontends

```
   human ──clicks──▶  window (React)  ──┐
                                        ├─▶  AppState  ──▶  the only truth
   agent ──`<cli> …`▶  CLI role      ──┘
```

Both surfaces call the same methods on the same `AppState`, so they cannot drift. The
window sees an agent's write as a pushed snapshot; the agent sees a human's edit as a
signal, and reads the detail with its own CLI.

`AppState` is pure state and logic — no I/O, no networking, no platform code. That is what
makes it testable, and it is why the tests are where the rules actually live.

**One binary, two roles.** `<cli> app` is the window Clatch launches; `<cli> <verb>` is the
agent's CLI. `clappkit::role` decides which at startup, so a clapp ships one executable.

## Two channels

Keeping these apart is the whole trick:

| | GUI↔CLI | control pipe |
|---|---|---|
| module | `clappkit::ipc` | `clappkit::control` |
| server | **your app** | **Clatch** |
| address | your own socket, under the app's data dir | `CLATCH_CONTROL_ADDR` (injected) |
| wire | newline JSON, yours to define | JSON-RPC 2.0 — see [`protocol.md`](protocol.md) |
| Clatch sees it? | no | yes |

`<cli> status` travels the left channel; Clatch never sees it. Signals travel the right.

## Run only under Clatch

`clappkit` calls `clatch_init` first thing in `app` mode:

- `CLATCH_INSTANCE_TOKEN` present → continue, then register. A mismatched `CLATCH_APP_ID`
  is a hard error.
- `CLATCH_STANDALONE=1` → continue with no launcher. The dev hatch.
- neither → `exec clatch run <appId>` and exit, so a bare double-click routes back through
  Clatch and the *installed* copy runs.

The launch command must never scrub `CLATCH_*` from the environment or the guard breaks.

## Signals

A signal is a fire-and-forget notice carrying no durable state — the agent reads the real
state through your CLI. Declare each one in `clatch.json` (`connector.signals`); the
declaration is the authority, and Clatch drops a signal whose type disagrees with it.

| type | effect |
|---|---|
| `run` | starts a turn on an **idle** agent. A busy one queues it — there is no preemption |
| `context` | queued in order, injected at the agent's next turn |
| `buffered` | one slot, latest wins; rides the user's next prompt |

Two behaviours explain most "my signal vanished" reports, both detailed in
[`protocol.md`](protocol.md):

- **The cut matrix.** A `run` signal only wakes an agent that was granted the app *and*
  whose bind is run-open. **Every bind is born all-open, `run` included**; narrowing is
  the user's act in the cut matrix, never a default.
- **Fan-out is all-or-nothing.** If any receiver has no room, the whole emission is
  refused and Clatch reports `app.toAgentRefused`. Surface that to the human — a
  full-inbox agent otherwise reads as a dead button.

Only **user** actions signal. The agent already knows about its own writes.

**Target ids, never names.** An empty target broadcasts; a non-empty one is still
intersected with the cut matrix, so targeting narrows and never widens. The app can target
precisely because Clatch injects `CLATCH_AGENT_ID` into the calling agent's shell, so the
app knows who invoked it — and an id survives a rename.

## Always-on apps

Clatch ships no cron, no scheduler and no app autostart — no clapp is started at boot.
A timer or observer app is an ordinary clapp that, **while it is running**, keeps its own
loop and emits a `run` signal when it fires. Missed-schedule catch-up and persistence are
your policy. `clock-clapp` is this pattern.

## Where each concern lives

| Concern | Where | Fork it? |
|---|---|---|
| Identity: id, cli, launch, signals | `clatch.json` | **edit** |
| Role dispatch, bootstrap, both channels, paths, atomic writes, media | `clappkit` | no — shared crate |
| Your state and rules | `src-tauri/src/state.rs` | **replace** |
| Tauri wiring, control pipe, pollers | `src-tauri/src/app.rs` | adapt |
| The agent's CLI and its manual | `src-tauri/src/cli.rs` | **adapt verbs** |
| The window | `src/` (React + TS) | **replace** |
| The one seam to the core | `src/bridge.ts` | keep the shape |

Identity is read from `clatch.json` at runtime, never duplicated into the code — so a fork
edits one file and everything follows.

## The look

Each clapp wears **its own** brand: WhatsApp's green, Telegram's blue, the tokens in that
app's `src/styles.css`. There is no shared theme to inherit and nothing to keep in sync.
What is shared is the shape — a window that is setup and status, never a second chat
surface. The agent handles the conversation; the window is where a human links an account,
sees what is connected, and reads what happened.
