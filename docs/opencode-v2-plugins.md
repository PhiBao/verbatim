# OpenCode v2 plugins — root cause, evidence, and what is left to do

Written 2026-10-10 while fixing "cannot start plugin in fresh session". Everything
below was measured on this machine against `opencode v2.0.26` (the binary at
`~/.opencode/bin/opencode`). No step below is inferred from documentation alone.

---

## 1. The short version

There are **two separate bugs**, and the previous session fixed the visible one and
triggered the invisible one.

| # | Symptom | Cause | Status |
|---|---------|-------|--------|
| 1 | `Plugin must export a default definition with an id and an effect or setup function` | Both plugins were v1 files that exported a bare function. v2 wants a default-export **object**. | Fixed in `abffc15` — but see #2 |
| 2 | Server hangs forever during plugin load; session never becomes usable | `{ id, effect: <plain async fn> }` passes validation, then **deadlocks server boot** | **Not fixed.** This is what `plugins.__off` was working around |

The trap: v2's loader *schema* accepts `effect`, and `scripts/check-plugins.ts`
checks for `effect` **or** `setup`. So the fix in `abffc15` looks correct, passes
the repo's own check, and still hangs the editor. `effect` is the internal
Effect-based variant used by built-ins; the **public plugin contract is `setup`**.

---

## 2. Proof

Reproduced in throwaway projects under `/tmp/opencode`, not in this repo, so the
repo state was never at risk.

```bash
# control: no plugin at all
cd /tmp/opencode/probe-b && timeout 120 opencode run --standalone \
  --model opencode/space-bunny-free "say pong"      # exit 0 in ~3.2s

# same project + a one-line .ts plugin
printf 'export default { id: "probe", effect: async () => {} }\n' \
  > .opencode/plugins/probe.ts
cd /tmp/opencode/probe-a && timeout 120 opencode run --standalone \
  --model opencode/space-bunny-free "say pong"      # exit 124 (HUNG), no output
```

The same one-line plugin with `setup` instead of `effect`: **exit 0**.

| variant | `opencode run --standalone` |
|---|---|
| no plugin | exit 0, 3.2s |
| `{ id, effect: async () => {} }` | **exit 124 — hang** |
| `{ id, effect: async () => () => {} }` | **exit 124 — hang** |
| `{ id, effect: async () => ({ event: async () => {} }) }` | **exit 124 — hang** |
| `{ id, setup: async () => {} }` | exit 0 |

It is the **field name**, not the body. It reproduces in a clean directory, with a
clean `XDG_CONFIG_HOME`, and with a fresh `XDG_DATA_HOME` (so it is not the 3.6 GB
`~/.local/share/opencode/opencode.db`).

### Where it stalls

The plugin module imports fine and its `effect` is invoked — proven by a probe
plugin that appends to a file at module scope and inside `effect`. Both marks
appear. The server then goes idle forever:

```
location services booted directory=… durationMs=88
loading plugin id=…/.opencode/plugins/probe.ts entrypoint=file://…
watcher started  path=…/probe.ts type=file backend=node
git remote get-url origin
git rev-list --max-parents=0 HEAD
<nothing — no AGENTS.md/skills watchers, no provider.updated, no model.updated>
```

Without a plugin the same log continues straight through to `provider.updated` /
`model.updated` and the client starts. All threads sit in `futex`/`epoll` — an
unresolved promise, not a busy loop, not a subprocess, not a socket.

---

## 3. The actual v2 contract

Source of truth, not the docs: `npm pack @opencode/plugin@2.0.26`, then
`dist/promise/plugin.d.ts`:

```ts
export type Cleanup = () => Promise<void> | void
export interface Plugin {
  readonly id: string
  readonly setup: (context: Context) => Promise<Cleanup | void> | Cleanup | void
}
```

`effect` appears nowhere in the public type. Confirmed at runtime by an
instrumented `{ id, setup }` plugin:

```
ctx keys: app,location,options,agent,aisdk,command,event,experimental,generate,
          model,provider,integration,mcp,permission,plugin,reference,rpc,skill,
          storage,tool,vcs,websearch,worktree,session,shell
location: {"directory":"/tmp/…","project":{"id":"global","directory":"/tmp/…",…}}
app: {"name":"cli","version":"2.0.26","channel":"latest"}
CLEANUP called        ← the returned cleanup DOES run on shutdown
```

v1's `{ event, "tool.execute.before", "chat.message",
"experimental.chat.system.transform", dispose }` hooks object **does not exist in
v2**. The equivalents are `ctx.event.subscribe()` (an `AsyncIterable`) and domain
hooks such as `ctx.tool.hook("execute.before", cb)`.

Observed event stream for one `opencode run` turn:

```
session.inbox.enqueued → session.execution.started → session.instructions.updated
→ session.inbox.delivered → session.step.started → session.text.started
→ session.text.delta* → session.text.ended → session.step.streamed
→ session.step.ended → session.usage.updated {cost, tokens}
→ session.execution.succeeded → location.shutdown → (Cleanup runs)
```

`session.usage.updated` carries `cost` and `tokens` — that is the metering signal
the neatlogs tracer wanted from `message.updated`.

---

## 4. Per-plugin verdict

Both files were re-tested **individually** as `{ id, setup: <existing body> }`:

| plugin | loads | session completes | actually functions |
|---|---|---|---|
| `entire.ts` | yes | yes (4.1s) | **no** |
| `neatlogs.ts` | yes | yes (3.9s) | **no** |
| both together | yes | yes | **no** |

They load and are **inert**. Each returns a v1 hooks object where v2 expects a
`Cleanup`, and v2 never reads it. So neither integration has ever run on v2 —
which matches the previous session's own commit message ("Neither plugin had ever
loaded").

### `entire.ts`
Generated by `entire enable --agent opencode`. Entire CLI **0.11.4** is installed
at `~/.local/bin/entire`, and its embedded template is still v1 — `strings` on the
binary shows the same `EntirePlugin` body, and it references v1-only events
(`message.updated`, `server.instance.disposed`). Re-running `entire enable` will
overwrite any hand-written file with a v1 one that hangs the editor again.
A v2 port means mapping `session.inbox.enqueued` / `session.execution.started` →
`turn-start`, `session.execution.succeeded|failed|interrupted` / `session.idle` →
`turn-end`, and the plugin's own **Cleanup** → `session-end` (v1 needed the
`server.instance.disposed` hack for this; v2 gives it to you).
Same `entire hooks opencode <name>` JSON payloads can be reused unchanged.

### `neatlogs.ts`
Wraps `NeatlogsOpencodePlugin` from `neatlogs@1.1.28`
(`node_modules/neatlogs/dist/opencode-plugin.mjs`). That plugin is v1-only —
`chat.message`, `tool.execute.*`, `experimental.chat.system.transform`, and a
`handleEvent` switch over `message.part.updated` / `message.updated` /
`session.idle`. None of those exist in v2, so the vendored plugin cannot work as
shipped; a v2 version has to be written against `ctx.event.subscribe()` and
`ctx.tool.hook(...)`.

Two things to know before anyone retries it:

- **`.env` is not loaded into the plugin process.** Measured:
  `process.env.NEATLOGS_API_KEY === null` inside `setup`, while
  `/home/kiter/neatlogs/.env` defines it. The vendored shipper then hits
  `if (!this.apiKey) { this.queue = []; return }` and **silently drops every
  span**. Any port must read the key explicitly (or document that it must be
  exported), otherwise it will look like it works.
- The engine already traces to neatlogs natively —
  `crates/engine/src/trace.rs` POSTs to `POST /v1/trace`, and
  `crates/engine/src/config.rs` reads `NEATLOGS_WRITE_KEY` / `NEATLOGS_API_KEY`.
  An opencode-level tracer is the "traced build sessions" rubric line, not core
  product behaviour. It is the piece to cut first if time is short.

---

## 5. Repo state right now

- `.opencode/plugins/` → renamed to `.opencode/plugins.__off` by the previous
  session. That is why the editor works today. Restore with
  `mv .opencode/plugins.__off .opencode/plugins` once the files are v2-shaped.
- `git status` shows ~9,300 deleted files under `.opencode/plugins/node_modules`
  — **`node_modules` is committed to git in this repo.** It should be ignored;
  it is also why the plugins directory is enormous.
- `scripts/check-plugins.ts` is actively harmful: it asserts
  `typeof def.effect === "function" || typeof def.setup === "function"`, i.e. it
  passes the exact shape that deadlocks the server. It should assert `setup` only,
  and ideally be replaced by a boot smoke test
  (`opencode run --standalone … "say pong"` must exit 0) — shape checks cannot
  catch this class of bug.

## 6. Suggested order of work

1. Delete or rewrite `scripts/check-plugins.ts` first, so nothing else is
   "verified" against the wrong contract.
2. Port `entire.ts` to `{ id, setup }`. Verify with a real run:
   `entire session list`, `entire checkpoint list`, `entire why <file>`.
3. Decide on `neatlogs.ts`: write the v2 tracer against
   `POST /v1/trace` (mirroring `crates/engine/src/trace.rs`), or drop the
   opencode-level plugin and rely on the engine. Do not leave the v1 wrapper in
   place — it loads and does nothing, which is worse than absent.
4. Un-ignore `node_modules`, drop it from git.
5. Only then `mv .opencode/plugins.__off .opencode/plugins` and re-verify with a
   real session.
