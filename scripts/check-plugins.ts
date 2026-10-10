#!/usr/bin/env bun
// Verifies the OpenCode plugins load with the shape its loader requires.
//
// OpenCode v2's plugin loader (see the PluginModule.load path in the opencode
// binary) validates the default export against:
//
//   { default: { id: string, effect: (…) => any } }
//   { default: { id: string, setup:  (…) => any } }
//
// A bare function fails with "must export a default definition with an id and an
// effect or setup function", and a module that throws while importing fails with
// the generic "Plugin failed to load". Both plugins here hit the first failure,
// which is why no Entire checkpoint was ever captured.
//
// `opencode run` cannot be used to check this: it produces no output in this
// environment with or without the plugins present, so the loader's own error is
// the only signal, and it only surfaces in the TUI.

import { readdir } from "node:fs/promises";
import { join, resolve } from "node:path";

const dir = ".opencode/plugins";
const files = (await readdir(dir)).filter((f) => f.endsWith(".ts"));

let failed = false;
for (const file of files) {
  const path = join(dir, file);
  try {
    const mod = await import(resolve(path));
    const def = mod.default;
    const ok =
      def &&
      typeof def === "object" &&
      typeof def.id === "string" &&
      def.id.length > 0 &&
      (typeof def.effect === "function" || typeof def.setup === "function");
    if (ok) {
      console.log(`ok    ${file}  id=${def.id}  ${def.effect ? "effect" : "setup"}=function`);
    } else {
      failed = true;
      console.log(`FAIL  ${file}  default export is not { id, effect } or { id, setup }`);
    }
  } catch (e) {
    failed = true;
    console.log(`FAIL  ${file}  threw while importing: ${e.message}`);
  }
}

process.exit(failed ? 1 : 0);
