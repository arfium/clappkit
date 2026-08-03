// The front-end half: plain .ts behind a Vite alias, so no npm package and no new
// dependency. React and @tauri-apps/api come from the consuming app's node_modules.
//
// Here: the two channels, the avatar cache, the snapshot wiring. Not here: an app's own
// types and normalisation — each bridge.ts keeps those and re-exports from "@clappkit".

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

/** The one webview→core call. Every clapp's command envelope goes through `run_cmd`. */
export async function cmd<S, R = unknown>(req: R): Promise<S> {
  return (await invoke("run_cmd", { req })) as S;
}

/** The one core→webview push. Every clapp's snapshot arrives on the `state` event —
 *  the same name `clappkit::app::STATE_EVENT` emits, and the reason it is a constant on
 *  both sides. */
export function onState<S>(cb: (s: S) => void): Promise<UnlistenFn> {
  return listen<S>("state", (e) => cb(e.payload));
}

// MARK: - Avatar images
//
// Mirrors Swift's `AvatarCache`: read once per path, keep it forever. The cache lives at
// module scope so a re-render never re-reads a file, and concurrent mounts of the same
// path share one in-flight request. The paths come from the roster and are ABSOLUTE FILE
// PATHS — the webview cannot open them, so Rust reads them (`clappkit::app::asset`).

const assetCache = new Map<string, string | null>();
const assetPending = new Map<string, Promise<string | null>>();
/** Everyone waiting on ANY path — one re-render per component when a load resolves. */
const assetWaiters = new Set<() => void>();

function loadAsset(path: string): Promise<string | null> {
  if (assetCache.has(path)) return Promise.resolve(assetCache.get(path) ?? null);
  const hit = assetPending.get(path);
  if (hit) return hit;
  const p = invoke<string | null>("asset", { path })
    .then((v) => v ?? null)
    .catch(() => null)
    .then((v) => {
      // Even a failure is cached (as null): a missing file is asked for exactly once.
      assetCache.set(path, v);
      assetPending.delete(path);
      for (const w of assetWaiters) w();
      return v;
    });
  assetPending.set(path, p);
  return p;
}

/**
 * Warm the cache for paths the UI is ABOUT to need — every roster avatar arrives in the
 * snapshot long before that agent is drawn, so by the time the strip renders one the
 * data: URI is already in hand and the monogram never flashes first.
 */
export function prefetchAssets(paths: (string | null | undefined)[]): void {
  for (const p of paths) {
    if (p && p.length > 0 && !assetCache.has(p) && !assetPending.has(p)) void loadAsset(p);
  }
}

/** Resolve an absolute file path to a data: URI (null while loading, or on failure). */
export function useAsset(path?: string | null): string | null {
  const key = path && path.length > 0 ? path : "";
  const [, bump] = useState(0);

  useEffect(() => {
    if (!key || assetCache.has(key)) return;
    // Subscribe for the whole mount, not per path: a load that resolves between this
    // render and the next `key` change still repaints exactly once.
    const wake = () => bump((n) => n + 1);
    assetWaiters.add(wake);
    void loadAsset(key);
    return () => {
      assetWaiters.delete(wake);
    };
  }, [key]);

  // Read-through: once cached, EVERY later render is synchronous — no refetch, no flash.
  return key ? assetCache.get(key) ?? null : null;
}

// MARK: - The snapshot wiring
//
// Two writers feed the window: the return value of our own `run_cmd`, and the `state`
// event the core pushes on its own (a scheduler tick, an inbound message, an agent's
// move). They RACE — an invoke response that resolves late is OLDER than an event that
// has already been applied — so every snapshot carries a monotonic `rev` and the loser is
// dropped. Only chess had this guard; the other three had the same race unguarded.

/** The minimum a snapshot must carry for the wiring to do its job. */
export type Snapshotish = { ok?: boolean; rev?: number };

export type SnapshotOptions<S> = {
  /** Fold a raw snapshot into the app's shape before it is stored (clock's `normalize`).
   *  App-specific on purpose; everything else here is not. */
  normalize?: (raw: S) => S;
  /** Ask for the first snapshot on mount. The command every clapp answers with its state. */
  initial?: unknown;
};

/**
 * The mount effect every clapp's `App()` wrote by hand: subscribe to `state`, ask for the
 * first snapshot, unsubscribe cleanly (including when the effect is torn down BEFORE
 * `listen` resolves — otherwise the listener leaks), and discard any snapshot older than
 * the one already held. A `{ ok: false }` reply is dropped, keeping the last good state.
 */
export function useSnapshot<S extends Snapshotish, R = unknown>(
  empty: S,
  options: SnapshotOptions<S> = {},
): { state: S; run: (req: R) => void; apply: (next: S) => void } {
  const [state, setState] = useState<S>(empty);
  const rev = useRef(-1);
  // `options` is almost always an inline literal, so its identity changes every render.
  // Holding it in a ref keeps `apply` and the subscribe effect STABLE — otherwise the
  // effect would tear down and re-`listen` on every render, which is both a leak and a
  // window in which pushed snapshots are missed.
  const opts = useRef(options);
  opts.current = options;

  const apply = useCallback((next: S) => {
    if (!next || next.ok === false) return;
    const r = typeof next.rev === "number" ? next.rev : 0;
    if (r < rev.current) return; // an older snapshot must never win
    rev.current = r;
    const n = opts.current.normalize;
    setState(n ? n(next) : next);
  }, []);

  useEffect(() => {
    let dead = false;
    let un: UnlistenFn | undefined;
    onState<S>(apply)
      .then((f) => {
        if (dead) f();
        else un = f;
      })
      .catch(() => {});
    cmd<S>(opts.current.initial ?? { cmd: "state" })
      .then(apply)
      .catch(() => {});
    return () => {
      dead = true;
      un?.();
    };
  }, [apply]);

  const run = useCallback(
    (req: R) => {
      cmd<S, R>(req).then(apply).catch(() => {});
    },
    [apply],
  );

  return { state, run, apply };
}

// MARK: - Agent avatar tint
//
// djb2 over the id's UTF-8 bytes in 64-bit SIGNED two's-complement arithmetic — exactly
// what Swift's `Int` does with `<<` and `&+`. A 32-bit `|0` djb2 picks different colours,
// so an app that reimplements this does not match the rest of the family.

const AGENT_TINTS = ["#45548C", "#267369", "#784D82", "#996138", "#4D6178"];

/** A stable colour for an agent id — the monogram behind a missing avatar. */
export function agentTint(id: string): string {
  const bytes = new TextEncoder().encode(id);
  let h = 5381n;
  for (const b of bytes) {
    h = BigInt.asIntN(64, BigInt.asIntN(64, h << 5n) + h);
    h = BigInt.asIntN(64, h + BigInt(b));
  }
  const abs = h < 0n ? -h : h;
  return AGENT_TINTS[Number(abs % BigInt(AGENT_TINTS.length))];
}
