#!/usr/bin/env node
// xcpc-server [workspace] [--port 4517] [--host 0.0.0.0] [--token <t>]
import { parseArgs } from "node:util";
import { start } from "../src/serve.js";

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    port: { type: "string", short: "p", default: "4517" },
    host: { type: "string", default: "127.0.0.1" },
    token: { type: "string" },
    web: { type: "string" },
  },
});

const workspace = positionals[0] ?? process.env.XCPC_WORKSPACE;
const s = await start({ workspace, port: Number(values.port), host: values.host, token: values.token, web: values.web });
console.log(`XCPC Workstation  ${s.url}`);
for (const u of s.lanUrls) console.log(`  on your network  ${u}`);
