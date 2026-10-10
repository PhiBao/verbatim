// Entire plugin for OpenCode — **v2 contract**.
//
// Why this file exists rather than the one `entire enable` generates:
//
//   `entire enable --agent opencode` (CLI 0.11.4) writes a **v1** plugin. It
//   exports a bare function, which OpenCode v2 rejects with "must export a
//   default definition with an id and an effect or setup function". Wrapping it
//   as `{ id, effect }` satisfies the loader's internal schema and then deadlocks
//   server boot forever, because `effect` is the Effect-based variant used by
//   built-ins and the public contract is `{ id, setup }`.
//
// So: this file is written against the v2 contract directly. Re-running
// `entire enable` will overwrite it with the broken v1 file — copy this back
// afterwards, or run scripts/plugin-smoke-test.sh to catch it.
//
// Contract source: @opencode/plugin@2.0.26 dist/promise/plugin.d.ts
//
//     interface Plugin {
//       readonly id: string
//       readonly setup: (context: Context) => Promise<Cleanup | void> | Cleanup | void
//     }
//
// Event shapes below were observed, not inferred (docs/opencode-v2-plugins.md §3):
//
//     session.inbox.enqueued       { sessionID, inboxID, item: { payload: { text } } }
//     session.execution.started    { sessionID }
//     session.execution.succeeded  { sessionID }
//
// The hook payloads sent to `entire hooks opencode <name>` are unchanged from v1.

import { spawn, spawnSync } from "node:child_process";

const PLUGIN_ID = "entire";

// Deliberately untyped against @opencode-ai/plugin: the installed version's
// `Plugin` is the v1 function signature, while v2 defines it as the object type
// `{ id, setup }`. Importing it here would type this file against the contract it
// is fixing. The shape is enforced by scripts/plugin-smoke-test.sh instead, which
// is the only check that catches the deadlock.
type Context = { directory: string; event: { subscribe: () => AsyncIterable<unknown> } };

/** Runs `entire hooks opencode <hook>`, exiting 0 when the CLI is absent. */
function hookCmd(hookName: string): string[] {
  return [
    "sh",
    "-c",
    `if ! command -v entire >/dev/null 2>&1; then exit 0; fi; exec entire hooks opencode ${hookName}`,
  ];
}

const setup = async (ctx: Context) => {
  const { directory } = ctx;
  let currentSessionID: string | null = null;
  let pendingPrompt = "";

  // Synchronous on purpose. `opencode run` tears down its event loop as soon as
  // execution settles, so an async hook fired on that event is killed before it
  // completes. turn-start must also finish before a fast mid-turn commit reaches
  // git hooks, or the checkpoint has no session to attach to.
  function callHookSync(hookName: string, payload: Record<string, unknown>) {
    try {
      const [cmd, ...args] = hookCmd(hookName);
      spawnSync(cmd, args, {
        cwd: directory,
        input: JSON.stringify(payload) + "\n",
        stdio: ["pipe", "ignore", "ignore"],
      });
    } catch {
      // Never throw: a plugin failure must not take down the editor.
    }
  }

  async function callHook(hookName: string, payload: Record<string, unknown>) {
    try {
      const [cmd, ...args] = hookCmd(hookName);
      await new Promise<void>((resolve) => {
        const proc = spawn(cmd, args, {
          cwd: directory,
          stdio: ["pipe", "ignore", "ignore"],
        });
        proc.on("error", () => resolve());
        proc.on("close", () => resolve());
        proc.stdin?.end(JSON.stringify(payload) + "\n");
      });
    } catch {
      // Never throw.
    }
  }

  function trackSession(id: string | undefined) {
    if (!id || currentSessionID === id) return;
    // A new session begins; drop the previous session's prompt so it cannot be
    // attributed to the wrong turns.
    currentSessionID = id;
    pendingPrompt = "";
  }

  // The event stream is consumed in the background and deliberately NOT awaited.
  // `setup` must return promptly: holding it open on an infinite iteration is
  // the shape of the deadlock this file exists to avoid.
  void (async () => {
    try {
      for await (const ev of ctx.event.subscribe()) {
        const data = (ev as { data?: Record<string, unknown> }).data ?? {};
        const sessionID = data.sessionID as string | undefined;

        switch ((ev as { type?: string }).type) {
          case "session.inbox.enqueued": {
            trackSession(sessionID);
            const item = data.item as { payload?: { text?: string } } | undefined;
            if (typeof item?.payload?.text === "string") {
              pendingPrompt = item.payload.text;
            }
            break;
          }

          case "session.execution.started": {
            trackSession(sessionID);
            if (sessionID) {
              callHookSync("turn-start", {
                session_id: sessionID,
                prompt: pendingPrompt,
                model: "",
              });
            }
            break;
          }

          case "session.execution.succeeded":
          case "session.execution.failed":
          case "session.execution.interrupted": {
            if (sessionID) {
              callHookSync("turn-end", { session_id: sessionID, model: "" });
            }
            break;
          }

          default:
            break;
        }
      }
    } catch {
      // The stream can end abruptly on shutdown; there is nothing to recover.
    }
  })();

  // v1 needed a `server.instance.disposed` hack for this, because `session.deleted`
  // only fires on explicit deletion and never on quit. v2 hands the plugin a
  // Cleanup, which runs on shutdown — that is the correct place for session-end.
  return async () => {
    if (currentSessionID) {
      callHookSync("session-end", { session_id: currentSessionID });
    }
  };
};

export default {
  id: PLUGIN_ID,
  setup,
};
