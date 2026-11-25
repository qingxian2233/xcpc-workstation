// One typesetting session per (problem|contest, id, editorial), kept warm so recompiles stay
// incremental. Compiles on a session are serialized and coalesced: while one runs, only the
// newest request waits; the ones it replaced resolve as `{ superseded: true }`.

const MAX_SESSIONS = 4;

class Entry {
  constructor(session) {
    this.session = session;
    this.seq = 0;
    this.rev = 0;
    this.tail = Promise.resolve();
  }

  compile(unsaved) {
    const seq = ++this.seq;
    const run = this.tail.then(async () => {
      if (seq !== this.seq) return { superseded: true };
      const report = await this.session.compile(unsaved);
      if (report.pages != null) this.rev++;
      return { ...report, rev: this.rev };
    });
    this.tail = run.catch(() => {});
    return run;
  }

  async render(page, scale) {
    if (this.rev === 0) await this.compile(null);
    return this.session.render(page, scale);
  }

  async pdf() {
    // Export what is on disk, not whatever the editor last previewed.
    const report = await this.compile(null);
    if (!report.superseded && report.pages == null) {
      const first = report.diagnostics.find((d) => d.severity === "error");
      throw new Error(`[compile_failed] ${first?.message ?? "compile failed"}`);
    }
    return this.session.pdf();
  }

  get defaultOutput() {
    return this.session.defaultOutput;
  }

  locate(page, x, y) {
    return this.rev === 0 ? null : this.session.locate(page, x, y);
  }
}

export class Sessions {
  constructor(ws) {
    this.ws = ws;
    /** @type {Map<string, Entry>} insertion order = recency */
    this.map = new Map();
  }

  get({ kind, id, editorial }) {
    const key = `${kind}:${id}:${editorial ? 1 : 0}`;
    let entry = this.map.get(key);
    if (entry) {
      this.map.delete(key);
    } else {
      entry = new Entry(this.ws.session(kind, id, { editorial }));
      if (this.map.size >= MAX_SESSIONS) this.map.delete(this.map.keys().next().value);
    }
    this.map.set(key, entry);
    return entry;
  }
}
