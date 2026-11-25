// Starts the server: API + the built web UI. Shared by the CLI and the Electron shell.
import { serve } from "@hono/node-server";
import { serveStatic } from "@hono/node-server/serve-static";
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { networkInterfaces } from "node:os";
import { createApp } from "./app.js";

const defaultWeb = join(dirname(fileURLToPath(import.meta.url)), "../../../apps/web/dist");

/**
 * @param {{ workspace?: string, port?: number, host?: string, token?: string | boolean, web?: string, onWorkspace?: (root: string) => void }} opts
 * `host` other than loopback requires a token (generated when `token` is true or unset).
 * @returns {Promise<{ url: string, lanUrls: string[], token?: string, close: () => void }>}
 */
export function start(opts = {}) {
  const host = opts.host ?? "127.0.0.1";
  const loopback = host === "127.0.0.1" || host === "localhost" || host === "::1";
  let token = typeof opts.token === "string" ? opts.token : undefined;
  if (!token && (!loopback || opts.token === true)) token = randomBytes(18).toString("base64url");

  const web = opts.web ?? defaultWeb;
  const app = createApp({
    workspace: opts.workspace,
    onWorkspace: opts.onWorkspace,
    token,
    static: existsSync(web)
      ? (app) => {
          const root = web;
          app.use("/assets/*", serveStatic({ root }));
          app.use("/*", serveStatic({ root }));
          // Read per request: a rebuilt UI must never be served a stale index.
          app.get("*", (c) => c.body(readFileSync(join(web, "index.html")), 200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-cache" }));
        }
      : undefined,
  });

  return new Promise((resolve) => {
    const server = serve({ fetch: app.fetch, port: opts.port ?? 0, hostname: host }, (info) => {
      const q = token ? `/?token=${token}` : "/";
      const url = `http://${loopback ? "127.0.0.1" : "localhost"}:${info.port}${q}`;
      const lanUrls = loopback
        ? []
        : Object.values(networkInterfaces())
            .flat()
            .filter((i) => i && i.family === "IPv4" && !i.internal)
            .map((i) => `http://${i.address}:${info.port}${q}`);
      resolve({
        url,
        lanUrls,
        token,
        port: info.port,
        close: () => {
          app.close();
          server.close();
        },
      });
    });
  });
}
