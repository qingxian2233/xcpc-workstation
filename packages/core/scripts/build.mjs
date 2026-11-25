// Builds crates/xcpc-node and copies the addon next to index.js as xcpc.<platform>-<arch>.node.
// `--debug` skips bundled fonts and optimization for a faster edit loop.
import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "../../..");
const debug = process.argv.includes("--debug");
const profile = debug ? "dev" : "node";
const args = ["build", "-p", "xcpc-node", "--profile", profile];
if (!debug) args.push("--features", "bundled-fonts");
if (!debug && !process.env.XCPC_FONT_DIRS) {
  const local = join(root, ".fonts");
  if (!existsSync(local)) {
    console.error("XCPC_FONT_DIRS is unset and .fonts/ is missing: run scripts/fetch-fonts.sh first");
    process.exit(1);
  }
  process.env.XCPC_FONT_DIRS = local;
}
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const lib = { linux: "libxcpc_node.so", darwin: "libxcpc_node.dylib", win32: "xcpc_node.dll" }[process.platform];
const out = join(root, "target", debug ? "debug" : "node", lib);
const dest = join(here, "..", `xcpc.${process.platform}-${process.arch}.node`);
copyFileSync(out, dest);
console.log(`→ ${dest}`);
