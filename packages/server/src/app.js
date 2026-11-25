// The HTTP face of the core: one Hono app that the browser, the Electron shell and phones
// on the LAN all talk to. No business logic here — it lives in the Rust crates.
import { Hono } from "hono";
import { streamSSE } from "hono/streaming";
import { getCookie, setCookie } from "hono/cookie";
import { timingSafeEqual } from "node:crypto";
import { Workstation, errorCode, version } from "@xcpc/core";
import { Sessions } from "./sessions.js";
import { watchWorkspace } from "./watch.js";

const STATUS = {
  not_found: 404,
  invalid: 400,
  invalid_id: 400,
  invalid_file: 422,
  path_escape: 403,
  disallowed_path: 403,
  read_only: 403,
  contest_frozen: 423,
  contest_archived: 423,
  conflict: 409,
  not_compiled: 409,
  no_workspace: 409,
  compile_failed: 422,
  too_many_problems: 422,
  problem_referenced: 409,
  already_frozen: 409,
  not_frozen: 409,
};

const MIME = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", svg: "image/svg+xml", webp: "image/webp" };

/**
 * @param {object} opts
 * @param {string} [opts.workspace]  workspace to open at start
 * @param {string} [opts.token]      required on every request when set (LAN mode)
 * @param {(app: Hono) => void} [opts.static]  mounts the web UI
 * @param {(root: string) => void} [opts.onWorkspace]  told whenever a workspace opens
 */
export function createApp(opts = {}) {
  /** @type {{ ws: Workstation | null, sessions: Sessions | null, stopWatch: (() => void) | null }} */
  const state = { ws: null, sessions: null, stopWatch: null };
  const listeners = new Set();
  const emit = (event) => listeners.forEach((fn) => fn(event));

  const open = (ws) => {
    state.stopWatch?.();
    state.ws = ws;
    state.sessions = new Sessions(ws);
    state.stopWatch = watchWorkspace(ws.root, (paths) => {
      ws.refresh();
      emit({ type: "changed", paths });
    });
    emit({ type: "workspace" });
    opts.onWorkspace?.(ws.root);
  };
  if (opts.workspace) {
    try {
      open(Workstation.open(opts.workspace));
    } catch (err) {
      console.warn(`cannot open ${opts.workspace}: ${err.message}`);
    }
  }

  const app = new Hono();

  app.onError((err, c) => {
    const code = errorCode(err) ?? "internal";
    const message = err.message.replace(/^\[[a-z_]+\]\s*/, "");
    if (code === "internal") console.error(err);
    return c.json({ code, message }, STATUS[code] ?? 500);
  });

  if (opts.token) {
    const expected = Buffer.from(opts.token);
    const ok = (t) => t && t.length === expected.length && timingSafeEqual(Buffer.from(t), expected);
    app.use("*", async (c, next) => {
      const fromQuery = c.req.query("token");
      if (ok(fromQuery)) {
        setCookie(c, "xcpc_token", fromQuery, { httpOnly: true, sameSite: "Strict", path: "/", maxAge: 60 * 60 * 24 * 30 });
        return next();
      }
      if (ok(getCookie(c, "xcpc_token")) || ok(c.req.header("x-xcpc-token"))) return next();
      return c.text("Forbidden", 403);
    });
  }

  const ws = () => {
    if (!state.ws) throw new Error("[no_workspace] no workspace open");
    return state.ws;
  };

  // ---- Workspace ---------------------------------------------------------------------------

  app.get("/api/workspace", (c) =>
    c.json(state.ws ? { root: state.ws.root, readOnly: state.ws.readOnly, version: version() } : { root: null, version: version() }),
  );
  app.post("/api/workspace/open", async (c) => {
    const { path, readOnly } = await c.req.json();
    open(Workstation.open(path, { readOnly: !!readOnly }));
    return c.json({ root: state.ws.root });
  });
  app.post("/api/workspace/example", async (c) => {
    const { path, language } = await c.req.json();
    open(Workstation.initExample(path, language));
    return c.json({ root: state.ws.root });
  });
  app.post("/api/workspace/init", async (c) => {
    const { path, template } = await c.req.json();
    open(Workstation.init(path, template));
    return c.json({ root: state.ws.root });
  });

  // ---- Library ------------------------------------------------------------------------------

  app.get("/api/problems", (c) => {
    const q = c.req.query();
    const query = {
      text: q.q || undefined,
      tags: q.tags ? q.tags.split(",") : undefined,
      ratingMin: q.min ? Number(q.min) : undefined,
      ratingMax: q.max ? Number(q.max) : undefined,
    };
    return c.json(ws().problems(query));
  });
  app.get("/api/problems/:id", (c) => c.json(ws().problem(c.req.param("id"))));
  app.put("/api/problems/:id/samples", async (c) => {
    ws().saveSamples(c.req.param("id"), await c.req.json());
    return c.json(ws().problem(c.req.param("id")).samples);
  });
  app.get("/api/contests", (c) => c.json(ws().contests()));
  app.get("/api/contests/:id", (c) => c.json(ws().contest(c.req.param("id"))));
  app.get("/api/search", (c) => c.json(ws().fuzzy(c.req.query("q") ?? "", 12)));

  // ---- Files --------------------------------------------------------------------------------

  app.get("/api/file", (c) => c.json(ws().read(c.req.query("path"))));
  app.put("/api/file", async (c) => {
    const { path, text, hash } = await c.req.json();
    const next = ws().write(path, text, hash ?? undefined);
    return c.json({ hash: next });
  });
  app.get("/api/asset", (c) => {
    const path = c.req.query("path");
    const ext = path.split(".").pop().toLowerCase();
    return c.body(ws().readBytes(path), 200, { "content-type": MIME[ext] ?? "application/octet-stream" });
  });

  // ---- Publishing ---------------------------------------------------------------------------

  const target = (c) => ({ kind: c.req.param("kind"), id: c.req.param("id"), editorial: c.req.query("editorial") === "1" });

  app.post("/api/compile/:kind/:id", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const result = await state.sessions.get(target(c)).compile(body.unsaved ?? null);
    return c.json(result);
  });
  app.get("/api/page/:kind/:id/:page", async (c) => {
    const page = Number.parseInt(c.req.param("page"), 10);
    const scale = Math.min(Math.max(Number(c.req.query("scale") ?? 2), 0.25), 6);
    const png = await state.sessions.get(target(c)).render(page, scale);
    // Keyed by `rev` in the URL, so a page never changes under a given address.
    return c.body(png, 200, { "content-type": "image/png", "cache-control": "private, max-age=31536000, immutable" });
  });
  app.get("/api/pdf/:kind/:id", async (c) => {
    const entry = state.sessions.get(target(c));
    const pdf = await entry.pdf();
    const name = entry.defaultOutput.split("/").pop();
    return c.body(pdf, 200, {
      "content-type": "application/pdf",
      "content-disposition": `${c.req.query("download") ? "attachment" : "inline"}; filename*=UTF-8''${encodeURIComponent(name)}`,
    });
  });
  app.get("/api/locate/:kind/:id/:page", (c) => {
    const { x, y } = c.req.query();
    return c.json(state.sessions.get(target(c)).locate(Number(c.req.param("page")), Number(x), Number(y)));
  });

  // ---- Live updates -------------------------------------------------------------------------

  app.get("/api/events", (c) =>
    streamSSE(c, async (stream) => {
      const send = (e) => stream.writeSSE({ event: e.type, data: JSON.stringify(e) });
      listeners.add(send);
      stream.onAbort(() => listeners.delete(send));
      while (!stream.aborted) {
        await stream.writeSSE({ event: "ping", data: "" });
        await stream.sleep(25_000);
      }
    }),
  );

  opts.static?.(app);
  app.close = () => state.stopWatch?.();
  return app;
}
