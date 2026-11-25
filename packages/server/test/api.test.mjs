// End-to-end over HTTP against a fresh copy of the built-in example workspace.
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Workstation } from "@xcpc/core";
import { start } from "../src/serve.js";

let dir, srv, base;
const get = (p) => fetch(base + p);
const json = async (p, init) => {
  const r = await fetch(base + p, init);
  return { status: r.status, body: await r.json() };
};
const send = (method, p, body) => json(p, { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });

before(async () => {
  dir = mkdtempSync(join(tmpdir(), "xcpc-test-"));
  Workstation.initExample(join(dir, "ws"), "zh");
  srv = await start({ workspace: join(dir, "ws"), port: 0, web: join(dir, "no-web") });
  base = new URL(srv.url).origin;
});
after(() => {
  srv.close();
  rmSync(dir, { recursive: true, force: true });
});

test("library lists the example problems and contests", async () => {
  const { body: problems } = await json("/api/problems");
  assert.ok(problems.length >= 3);
  assert.ok(problems.every((p) => p.id && p.title));
  const { body: contests } = await json("/api/contests");
  assert.ok(contests.length >= 1);
  const { body: filtered } = await json(`/api/problems?q=${encodeURIComponent(problems[0].title)}`);
  assert.equal(filtered[0].id, problems[0].id);
});

test("compiles a contest, renders a page and exports a PDF", async () => {
  const { body: contests } = await json("/api/contests");
  const id = contests[0].id;
  const { status, body: report } = await send("POST", `/api/compile/contest/${id}`, {});
  assert.equal(status, 200);
  assert.ok(report.pages > 1, JSON.stringify(report.diagnostics));
  assert.equal(report.pageSizes.length, report.pages);
  assert.ok(report.problems.length >= 1);

  const png = await get(`/api/page/contest/${id}/0?scale=1&rev=${report.rev}`);
  assert.equal(png.headers.get("content-type"), "image/png");
  const bytes = Buffer.from(await png.arrayBuffer());
  assert.equal(bytes.subarray(1, 4).toString(), "PNG");

  const pdf = Buffer.from(await (await get(`/api/pdf/contest/${id}`)).arrayBuffer());
  assert.equal(pdf.subarray(0, 5).toString(), "%PDF-");
});

test("unsaved text is typeset without touching the disk", async () => {
  const { body: problems } = await json("/api/problems");
  const p = (await json(`/api/problems/${problems[0].id}`)).body;
  const path = p.statements[0].path;
  const before = readFileSync(join(srv_root(), path), "utf8");
  const { body: report } = await send("POST", `/api/compile/problem/${p.id}`, { unsaved: { path, text: before + "\n\n![](missing-figure.png)\n" } });
  assert.ok(report.diagnostics.some((d) => d.file === path), JSON.stringify(report.diagnostics));
  assert.equal(readFileSync(join(srv_root(), path), "utf8"), before);
});

test("writes are conflict-checked", async () => {
  const { body: problems } = await json("/api/problems");
  const p = (await json(`/api/problems/${problems[1].id}`)).body;
  const path = p.statements[0].path;
  const { body: file } = await json(`/api/file?path=${encodeURIComponent(path)}`);
  const ok = await send("PUT", "/api/file", { path, text: file.text + "\n改动一\n", hash: file.hash });
  assert.equal(ok.status, 200);
  const stale = await send("PUT", "/api/file", { path, text: "覆盖", hash: file.hash });
  assert.equal(stale.status, 409);
  assert.equal(stale.body.code, "conflict");
});

test("paths cannot leave the workspace", async () => {
  const r = await json(`/api/file?path=${encodeURIComponent("../../etc/passwd")}`);
  assert.ok(r.status >= 400 && r.status < 500, String(r.status));
});

test("a token is required off loopback", async () => {
  const lan = await start({ workspace: srv_root(), port: 0, host: "0.0.0.0", web: join(dir, "no-web") });
  try {
    const origin = `http://127.0.0.1:${lan.port}`;
    assert.ok(lan.token);
    assert.equal((await fetch(`${origin}/api/workspace`)).status, 403);
    assert.equal((await fetch(`${origin}/api/workspace`, { headers: { "x-xcpc-token": lan.token } })).status, 200);
  } finally {
    lan.close();
  }
});

function srv_root() {
  return join(dir, "ws");
}
