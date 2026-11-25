// npm run dev [-- <workspace>]: API server on :4517 + Vite on :5173 with hot reload.
import { spawn } from "node:child_process";

const ws = process.argv[2] ?? process.env.XCPC_WORKSPACE ?? "";
const procs = [
  spawn(process.execPath, ["--watch-path=packages/server/src", "packages/server/bin/xcpc-server.mjs", ...(ws ? [ws] : []), "--web", "/nonexistent"], { stdio: "inherit" }),
  spawn("npm", ["-w", "@xcpc/web", "run", "dev"], { stdio: "inherit", shell: process.platform === "win32" }),
];
const stop = () => procs.forEach((p) => p.kill());
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
