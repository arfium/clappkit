//! The Clatch control pipe, app side — a **reactive** client. `clatch-pipe::Client` is
//! emit-then-`serve` (its `serve` borrows `&mut self`, so you cannot emit while it
//! runs). A real clapp emits from a background loop *during* serve: an alarm fires, a
//! Telegram DM arrives, a timer ends. So clappkit owns a small client over `clatch-ipc`'s
//! public `Peer`/`Inbox` — one `select!` loop that answers Clatch's callbacks
//! (ping / shutdown / agent roster) AND drains an emit channel, concurrently. The wire
//! is the frozen protocol (reference/protocol.md); the method names are `clatch-pipe`'s
//! own `wire` constants, so this stays in lockstep with Clatch.

use anyhow::{bail, Result};
use clatch_core::{SignalDecl, SignalType};
use clatch_ipc::{connect, FrameLimits, Inbox, Peer, Response};
use clatch_pipe::wire::method;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// How long the loop will wait for the pipe to accept one signal before giving up on it.
/// A bounded wait, not a lossy try-send: signals ARE the product (the alarm that wakes the
/// agent at 07:30), so a momentary hiccup must not drop one — while a wedged socket must
/// not stall the ping/shutdown arms either.
const EMIT_DELIVERY_TIMEOUT: Duration = Duration::from_secs(1);

/// How long an app's shutdown hook gets to flush before we exit anyway. Clatch has its own
/// kill timeout; a hook that hangs must not turn a clean quit into a kill.
const SHUTDOWN_HOOK_DEADLINE: Duration = Duration::from_secs(1);

/// One bound agent, from the latest `app.agents` roster push.
#[derive(Clone, Debug, Default)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub model: Option<String>,
    pub avatar: Option<Avatar>,
}

/// An agent's profile image (a same-machine file path Clatch resolved for us).
#[derive(Clone, Debug)]
pub struct Avatar {
    pub mime: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
}

/// The roster row a clapp shows and serialises — the projection all four apps hand-rolled
/// and no two of them agreed on.
///
/// Blank display fields are `None`, **never** `""`: Clatch sends an empty string when it
/// has nothing to say about a backend or a model, and rendering "" as if it were a value
/// is what put an empty model line in one app's GUI. `avatar` is an ABSOLUTE FILE PATH,
/// not bytes — resolve it for a webview with [`crate::asset::data_uri`].
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct AgentRow {
    pub id: String,
    pub name: String,
    pub backend: Option<String>,
    pub model: Option<String>,
    pub avatar: Option<String>,
}

impl AgentRow {
    /// Project one [`Agent`] the way every clapp meant to: empty strings become `None`.
    pub fn from_agent(a: Agent) -> AgentRow {
        AgentRow {
            id: a.id,
            name: a.name,
            backend: Some(a.backend).filter(|s| !s.is_empty()),
            model: a.model.filter(|s| !s.is_empty()),
            avatar: a.avatar.map(|av| av.path).filter(|s| !s.is_empty()),
        }
    }
}

/// One signal an app's PURE state layer wants sent: a declared signal id, the target agent
/// ids (empty = broadcast to every bound-and-uncut agent), and the payload.
///
/// The state layer stays sync and side-effect-free by RETURNING these; the caller drains
/// them into the live pipe with [`Control::emit_all`]. Four apps declared this same struct
/// with the same three fields and the same comment, because clappkit kept it private.
#[derive(Clone, Debug)]
pub struct Emit {
    pub id: String,
    /// Agent ids; `[]` = broadcast.
    pub target: Vec<String>,
    pub payload: Value,
}

/// Why the app is going away, handed to an [`OnShutdown`] hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownCause {
    /// Clatch asked, over the control pipe's `app.shutdown` callback.
    Requested,
    /// The pipe ended — EOF or an unreadable frame. The instance is gone either way.
    PipeClosed,
}

impl std::fmt::Display for ShutdownCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShutdownCause::Requested => write!(f, "Clatch asked us to stop"),
            ShutdownCause::PipeClosed => write!(f, "the control pipe closed"),
        }
    }
}

/// What to run when Clatch says shut down, or the control pipe ends — before the process
/// goes away. clappkit exits as soon as the hook returns (or after
/// [`SHUTDOWN_HOOK_DEADLINE`], whichever comes first), so use it to flush state, kill a
/// child process, or abort a poller; call [`std::process::exit`] inside it if you need to
/// control the code. It runs on its own thread, so blocking in it is fine.
pub type OnShutdown = Arc<dyn Fn(ShutdownCause) + Send + Sync>;

/// A cloneable handle to the live control pipe: emit declared signals from any task
/// while the background loop concurrently serves Clatch and tracks the roster.
#[derive(Clone)]
pub struct Control {
    emit_tx: mpsc::Sender<Emit>,
    agents: Arc<Mutex<Vec<Agent>>>,
    dropped: Arc<AtomicU64>,
    wired: bool,
}

impl Control {
    /// Connect + register, spawning the serve+emit loop. In the `CLATCH_STANDALONE` dev
    /// hatch (no `CLATCH_CONTROL_ADDR`) this returns a no-op handle so the GUI and CLI
    /// still work with signals silently dropped.
    pub async fn connect(declared: Vec<SignalDecl>) -> Result<Control> {
        Control::connect_with(declared, default_shutdown_hook()).await
    }

    /// [`Control::connect`] with a shutdown hook. The default hook only logs — correct for
    /// a stateless clapp, wrong for one that owns a Node sidecar, a poller, or an unflushed
    /// store, because the loop's exit path skips every destructor. Note the hook cannot
    /// fire in the standalone dev hatch: with no pipe there is no shutdown to hear.
    pub async fn connect_with(declared: Vec<SignalDecl>, on_shutdown: OnShutdown) -> Result<Control> {
        let agents = Arc::new(Mutex::new(Vec::new()));
        let dropped = Arc::new(AtomicU64::new(0));
        // 1024, not 256: clock's scheduler can fire a burst of alarms in one tick, and a
        // dropped signal is a wake the agent never gets.
        let (emit_tx, mut emit_rx) = mpsc::channel::<Emit>(1024);

        let addr = std::env::var("CLATCH_CONTROL_ADDR")
            .ok()
            .filter(|s| !s.is_empty());
        let Some(addr) = addr else {
            // Standalone: no pipe. Drain emits into the void so the app still runs.
            tokio::spawn(async move { while emit_rx.recv().await.is_some() {} });
            return Ok(Control { emit_tx, agents, dropped, wired: false });
        };

        let token = std::env::var("CLATCH_INSTANCE_TOKEN").unwrap_or_default();
        let stream = connect(&addr).await?;
        let (peer, inbox) = Peer::start(stream, addr, FrameLimits::control());

        // The per-instance socket names us; the token proves us. Register carries only it.
        let resp = peer
            .request(method::REGISTER, json!({ "instanceToken": token }), Duration::from_secs(5))
            .await?;
        if !resp.is_ok() {
            bail!("Clatch register rejected");
        }

        let types: HashMap<String, SignalType> =
            declared.into_iter().map(|d| (d.id, d.signal_type)).collect();
        if types.is_empty() {
            // Wired to a real Clatch with nothing declared: every emit will be refused
            // below, and the app would run perfectly while delivering nothing. That is a
            // packaging bug (see `crate::declared_signals`), so say it once, loudly.
            eprintln!(
                "clappkit: no signals declared — this app can emit NOTHING. \
                 Is clatch.json missing from the working directory?"
            );
        }
        tokio::spawn(serve_loop(peer, inbox, emit_rx, agents.clone(), types, on_shutdown));
        Ok(Control { emit_tx, agents, dropped, wired: true })
    }

    /// Emit a declared signal (fire-and-forget). `target` is a list of agent **ids**
    /// (empty = broadcast to every bound-and-uncut agent). The declared type is stamped
    /// by the loop; an undeclared id is refused there.
    ///
    /// If the emit queue is full the signal is dropped — and, unlike before, SAID SO:
    /// "the alarm just did not fire" is indistinguishable from a scheduler bug without a
    /// line in the log. [`Control::dropped_signals`] counts them for a status surface.
    pub fn emit(&self, id: &str, target: Vec<String>, payload: Value) {
        let e = Emit { id: id.to_string(), target, payload };
        if self.emit_tx.try_send(e).is_err() {
            let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            if n == 1 || n % 100 == 0 {
                eprintln!("clappkit: dropped signal '{id}' (emit queue full; {n} dropped)");
            }
        }
    }

    /// Emit a batch — the `for e in emits { control.emit(…) }` loop every clapp writes
    /// after applying a command.
    pub fn emit_all(&self, emits: impl IntoIterator<Item = Emit>) {
        for e in emits {
            self.emit(&e.id, e.target, e.payload);
        }
    }

    /// How many signals have been dropped for want of queue space. Zero is the normal
    /// answer; anything else belongs in the app's status output.
    pub fn dropped_signals(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// The current agent roster (latest `app.agents` push).
    pub fn agents(&self) -> Vec<Agent> {
        self.agents.lock().unwrap().clone()
    }

    /// The live roster as display-normalised [`AgentRow`]s — the projection all four apps
    /// hand-rolled, so they can never disagree again.
    pub fn roster(&self) -> Vec<AgentRow> {
        self.agents().into_iter().map(AgentRow::from_agent).collect()
    }

    /// `id` → that agent's current display name (`None` once it leaves the roster). Name
    /// is display-only and MAY change between pushes; `id` is the wire key you store.
    pub fn name_of(&self, id: &str) -> Option<String> {
        self.agents()
            .into_iter()
            .find(|a| a.id == id)
            .map(|a| a.name)
    }

    /// A shared read handle to the roster, kept fresh by the loop.
    pub fn agents_handle(&self) -> Arc<Mutex<Vec<Agent>>> {
        self.agents.clone()
    }

    /// Whether a real Clatch pipe is attached (false in the standalone dev hatch).
    pub fn is_wired(&self) -> bool {
        self.wired
    }
}

/// The hook [`Control::connect`] installs: log which of the two shutdown paths fired —
/// the line that was missing when apps died mid-flight for no stated reason — and let
/// clappkit exit.
fn default_shutdown_hook() -> OnShutdown {
    Arc::new(|cause: ShutdownCause| eprintln!("clappkit: shutting down — {cause}"))
}

fn type_str(t: &SignalType) -> &'static str {
    match t {
        SignalType::Run => "run",
        SignalType::Context => "context",
        SignalType::Buffered => "buffered",
    }
}

async fn serve_loop(
    peer: Peer,
    inbox: Inbox,
    mut emit_rx: mpsc::Receiver<Emit>,
    agents: Arc<Mutex<Vec<Agent>>>,
    types: HashMap<String, SignalType>,
    on_shutdown: OnShutdown,
) {
    let Inbox {
        mut requests,
        mut notifications,
    } = inbox;
    // Refusals are logged once per id: a signal emitted on every tick would otherwise
    // bury the log in the same line and hide everything else.
    let mut refused: HashSet<String> = HashSet::new();
    loop {
        tokio::select! {
            // Clatch → us: request/response callbacks (ping keeps us alive; shutdown ends us).
            req = requests.recv() => {
                let Some(req) = req else { break }; // pipe closed
                match req.method.as_str() {
                    m if m == method::PING => {
                        let _ = peer.respond(Response::ok(req.id, json!({ "ok": true }))).await;
                    }
                    m if m == method::SHUTDOWN => {
                        // FLUSHED, not queued: `respond` only awaits the enqueue, so the
                        // old code exited with the ack still in the pump's channel and
                        // Clatch fell back to its kill timeout. protocol.md § app.shutdown
                        // is "reply, then exit cleanly" — this is the reply half.
                        let _ = peer.respond_flushed(Response::ok(req.id, json!({}))).await;
                        run_hook_then_exit(&on_shutdown, ShutdownCause::Requested).await;
                    }
                    _ => { let _ = peer.respond(Response::ok(req.id, json!({}))).await; }
                }
            }
            // Clatch → us: fire-and-forget notifications (the roster; refusals).
            notif = notifications.recv() => {
                let Some(n) = notif else { break };
                if n.method == method::AGENTS {
                    if let Some(arr) = n.params.get("agents").and_then(Value::as_array) {
                        *agents.lock().unwrap() = arr.iter().map(parse_agent).collect();
                    }
                }
            }
            // Us → Clatch: an app.toAgent, stamped with the signal's declared type.
            emit = emit_rx.recv() => {
                let Some(emit) = emit else { break }; // every Control dropped
                match types.get(&emit.id) {
                    Some(ty) => {
                        let params = json!({
                            "id": emit.id, "type": type_str(ty),
                            "target": emit.target, "payload": emit.payload,
                        });
                        // Bounded await rather than a lossy try-send: give a momentary
                        // hiccup time to clear, but never park the ping arm on a wedged pipe.
                        if tokio::time::timeout(EMIT_DELIVERY_TIMEOUT, peer.notify(method::TO_AGENT, params))
                            .await
                            .is_err()
                        {
                            eprintln!("clappkit: timed out delivering signal '{}' to Clatch", emit.id);
                        }
                    }
                    None => {
                        if refused.insert(emit.id.clone()) {
                            eprintln!(
                                "clappkit: refusing to emit undeclared signal '{}' \
                                 (add it to clatch.json's connector.signals)",
                                emit.id
                            );
                        }
                    }
                }
            }
        }
    }
    // The pipe ended (EOF / bad frame). Under Clatch that means "instance gone": exit.
    run_hook_then_exit(&on_shutdown, ShutdownCause::PipeClosed).await;
}

/// Run the app's shutdown hook on its own thread, bounded, then end the process. The hook
/// may block (kill a child, fsync a store); a hook that hangs must not keep the app alive
/// past Clatch's patience, so the wait is capped.
async fn run_hook_then_exit(hook: &OnShutdown, cause: ShutdownCause) -> ! {
    let hook = hook.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        hook(cause);
        let _ = tx.send(());
    });
    if tokio::time::timeout(SHUTDOWN_HOOK_DEADLINE, rx).await.is_err() {
        eprintln!("clappkit: shutdown hook did not finish in time — exiting anyway");
    }
    std::process::exit(0);
}

fn parse_agent(v: &Value) -> Agent {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    Agent {
        id: s("id"),
        name: s("name"),
        backend: s("backend"),
        model: v.get("model").and_then(Value::as_str).map(String::from),
        avatar: v.get("avatar").and_then(|a| {
            let path = a.get("path").and_then(Value::as_str)?;
            Some(Avatar {
                mime: a.get("mime").and_then(Value::as_str).unwrap_or_default().to_string(),
                path: path.to_string(),
                width: a.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                height: a.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str, backend: &str, model: Option<&str>) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            backend: backend.into(),
            model: model.map(String::from),
            avatar: None,
        }
    }

    #[test]
    fn a_roster_row_turns_clatchs_empty_strings_into_none() {
        let row = AgentRow::from_agent(agent("a1", "Alice", "", Some("")));
        assert_eq!(row.backend, None, "an empty backend must not render as a value");
        assert_eq!(row.model, None, "an empty model must not render as a value");
        assert_eq!(row.avatar, None);
        assert_eq!(row.id, "a1");
        assert_eq!(row.name, "Alice");
    }

    #[test]
    fn a_populated_row_keeps_its_values_and_the_avatar_path() {
        let mut a = agent("a2", "Bob", "claude-code", Some("opus"));
        a.avatar = Some(Avatar {
            mime: "image/png".into(),
            path: "/tmp/bob.png".into(),
            width: 64,
            height: 64,
        });
        let row = AgentRow::from_agent(a);
        assert_eq!(row.backend.as_deref(), Some("claude-code"));
        assert_eq!(row.model.as_deref(), Some("opus"));
        assert_eq!(row.avatar.as_deref(), Some("/tmp/bob.png"));
    }

    #[test]
    fn an_empty_avatar_path_is_not_an_avatar() {
        let mut a = agent("a3", "Cy", "x", None);
        a.avatar = Some(Avatar { mime: String::new(), path: String::new(), width: 0, height: 0 });
        assert_eq!(AgentRow::from_agent(a).avatar, None);
    }

    #[test]
    fn a_row_serialises_to_the_shape_the_front_ends_expect() {
        let v = serde_json::to_value(AgentRow::from_agent(agent("a1", "Alice", "", None))).unwrap();
        assert_eq!(v["id"], "a1");
        assert_eq!(v["name"], "Alice");
        assert!(v["backend"].is_null());
        assert!(v["model"].is_null());
        assert!(v["avatar"].is_null());
    }

    #[tokio::test]
    async fn a_standalone_control_drops_signals_without_blocking_or_counting_them() {
        // The dev hatch: no CLATCH_CONTROL_ADDR, so the drain task swallows emits.
        let c = Control {
            emit_tx: {
                let (tx, mut rx) = mpsc::channel::<Emit>(4);
                tokio::spawn(async move { while rx.recv().await.is_some() {} });
                tx
            },
            agents: Arc::new(Mutex::new(vec![agent("a1", "Alice", "b", None)])),
            dropped: Arc::new(AtomicU64::new(0)),
            wired: false,
        };
        c.emit_all(vec![Emit { id: "alarm.fired".into(), target: vec![], payload: json!({}) }]);
        assert!(!c.is_wired());
        assert_eq!(c.dropped_signals(), 0);
        assert_eq!(c.name_of("a1").as_deref(), Some("Alice"));
        assert_eq!(c.name_of("nope"), None);
        assert_eq!(c.roster().len(), 1);
    }

    #[tokio::test]
    async fn a_full_emit_queue_is_counted_not_silent() {
        let (emit_tx, _rx) = mpsc::channel::<Emit>(1); // nobody drains it
        let c = Control {
            emit_tx,
            agents: Arc::new(Mutex::new(Vec::new())),
            dropped: Arc::new(AtomicU64::new(0)),
            wired: true,
        };
        for _ in 0..5 {
            c.emit("alarm.fired", vec![], json!({}));
        }
        assert_eq!(c.dropped_signals(), 4, "one fits in the channel, four are dropped");
    }

    #[test]
    fn the_roster_push_is_parsed_the_way_clatch_sends_it() {
        let a = parse_agent(&json!({
            "id": "a1", "name": "Alice", "backend": "claude-code", "model": "opus",
            "avatar": { "mime": "image/png", "path": "/tmp/a.png", "width": 128, "height": 128 }
        }));
        assert_eq!(a.id, "a1");
        assert_eq!(a.backend, "claude-code");
        assert_eq!(a.avatar.as_ref().unwrap().path, "/tmp/a.png");
        assert_eq!(a.avatar.as_ref().unwrap().width, 128);
        // A row with nothing but an id must not panic or invent values.
        let b = parse_agent(&json!({ "id": "b" }));
        assert_eq!(b.name, "");
        assert!(b.avatar.is_none());
        // An avatar without a path is not an avatar.
        let c = parse_agent(&json!({ "id": "c", "avatar": { "mime": "image/png" } }));
        assert!(c.avatar.is_none());
    }

    #[test]
    fn shutdown_causes_read_as_sentences() {
        assert!(ShutdownCause::Requested.to_string().contains("Clatch"));
        assert!(ShutdownCause::PipeClosed.to_string().contains("pipe"));
    }
}
