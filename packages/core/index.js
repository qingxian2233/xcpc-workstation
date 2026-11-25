// Loads the native addon for this platform and smooths over the few JSON-string returns.
import { createRequire } from "node:module";
import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const file = process.env.XCPC_CORE_ADDON ?? join(here, `xcpc.${process.platform}-${process.arch}.node`);
if (!existsSync(file)) {
  throw new Error(`@xcpc/core: no native addon at ${file}; run \`npm run build:core\``);
}
const native = createRequire(import.meta.url)(file);

const compile = native.Session.prototype.compile;
native.Session.prototype.compile = async function (unsaved) {
  return JSON.parse(await compile.call(this, unsaved ?? null));
};

/** `[conflict] …` → `"conflict"`; undefined for errors that did not come from the core. */
export function errorCode(err) {
  return /^\[([a-z_]+)\]/.exec(err?.message ?? "")?.[1];
}

export const { Workstation, Session, presets, version } = native;
