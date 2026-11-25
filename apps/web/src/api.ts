// Typed client for @xcpc/server. Shapes mirror packages/core/index.d.ts.

export interface Limits { time?: string; memory?: string; input?: string; output?: string }
export interface Source { platform: string; id: string; url?: string }

export interface ProblemSummary {
  id: string;
  title: string;
  rating?: number;
  tags?: string[];
  sources?: Source[];
  has_editorial: boolean;
  has_solutions: boolean;
  statement_path?: string;
}

export interface ProblemDetail {
  id: string;
  dir: string;
  meta: { title: string; rating?: number; tags?: string[]; author?: string; limits: Limits; source?: Source[] };
  statements: { language: "zh" | "en" | null; path: string; text: string }[];
  samples: { name: string; input: string; output: string }[];
  editorial: { path: string; text: string } | null;
}

export interface ContestItem { id: string; label: string; title?: string }
export interface ContestMeta {
  name: string;
  location?: string;
  date?: string;
  author?: string;
  frozen: boolean;
  problem: ContestItem[];
}
export interface ContestListItem { id: string; meta: ContestMeta; stats: Record<string, unknown> }
export interface ContestDetail {
  id: string;
  dir: string;
  meta: ContestMeta;
  items: { item: ContestItem; problem: { id: string; dir: string; title: string } | null }[];
}

export interface Diagnostic {
  severity: "error" | "warning";
  code?: string;
  message: string;
  file?: string;
  line?: number;
  column?: number;
  page?: number;
  hints?: string[];
}

export interface CompileReport {
  ok: boolean;
  pages?: number;
  problems: { label: string; id: string; first: number; count: number }[];
  diagnostics: Diagnostic[];
  compile_ms: number;
  pageSizes: [number, number][];
  rev: number;
  superseded?: undefined;
}
export type CompileResult = CompileReport | { superseded: true };

export interface FuzzyHit { kind: string; id: string; title: string; [k: string]: unknown }

export interface WorkspaceInfo { root: string | null; readOnly?: boolean; version: string }

export type ApiFormat = "chat" | "responses" | "messages";

export interface LlmSettings {
  apiFormat: ApiFormat;
  baseUrl: string;
  model: string;
  reasoningEffort: "auto" | "minimal" | "low" | "medium" | "high";
  apiKeyConfigured: boolean;
}

// ---- Problem setter ------------------------------------------------------------------------

export type TierId = "warmup" | "easy" | "medium" | "hard" | "challenge";
export interface Tier { id: TierId; label: string; min: number; max: number }

export interface Brief { background: string; tier: TierId | null; tags: string[]; notes: string }

export interface Killed { approach: string; how: string }
export interface Idea { title: string; summary: string; solution: string; complexity: string; killed: Killed[]; rating: number }

export interface DraftSample { input: string; output: string; explanation: string }
export interface WrongSolution { name: string; approach: string; code: string }
export interface Draft {
  slug: string;
  title: string;
  rating: number;
  tags: string[];
  limits: { timeMs: number; memoryMb: number };
  background: string;
  legend: string;
  input: string;
  output: string;
  constraints: string;
  note: string;
  samples: DraftSample[];
  editorial: string;
  std: string;
  brute: string;
  generator: string;
  wrong: WrongSolution[];
  idea: Idea;
}

export type CheckStatus = "pending" | "running" | "ok" | "warn" | "fail" | "skipped";
export interface Check { key: string; label: string; status: CheckStatus; detail: string }
export interface VerifyReport {
  ok: boolean;
  compiler: string;
  checks: Check[];
  samples: { output: string; claimedMatches: boolean }[];
  stressRuns: number;
  stdMaxMs: number;
  counterexample: { input: string; expected: string; got: string; who: string } | null;
}

export interface SetterEnv {
  compiler: { ok: true; version: string } | { ok: false; message: string };
  llm: { configured: boolean; model: string };
  tiers: Tier[];
}

export interface DraftResult {
  draft: Draft;
  report: VerifyReport | null;
  rounds: number;
  preview: CompileReport | { error: { code: string; message: string } };
  statement: string;
  /** Set by the client when it receives the result. */
  stamp?: number;
}

export interface Job<T> {
  done: boolean;
  stage: string;
  round: number;
  report: VerifyReport | null;
  result: T | null;
  error: { code: string; message: string } | null;
}

export type DocKind = "problem" | "contest";
export interface DocRef { kind: DocKind; id: string; editorial?: boolean }

export class ApiError extends Error {
  constructor(public code: string, message: string, public status: number) {
    super(message);
  }
}

async function call<T>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ code: "http", message: res.statusText }));
    throw new ApiError(err.code, err.message, res.status);
  }
  return res.json();
}

const q = (o: Record<string, string | number | undefined | boolean>) => {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(o)) if (v !== undefined && v !== "" && v !== false) p.set(k, String(v === true ? 1 : v));
  const s = p.toString();
  return s ? `?${s}` : "";
};

const docPath = (d: DocRef) => `${d.kind}/${encodeURIComponent(d.id)}`;

export const api = {
  workspace: () => call<WorkspaceInfo>("GET", "/api/workspace"),
  openWorkspace: (path: string) => call<{ root: string }>("POST", "/api/workspace/open", { path }),
  exampleWorkspace: (path: string, language: "zh" | "en") => call<{ root: string }>("POST", "/api/workspace/example", { path, language }),
  initWorkspace: (path: string) => call<{ root: string }>("POST", "/api/workspace/init", { path }),

  problems: (f: { q?: string; tags?: string[]; min?: number; max?: number } = {}) =>
    call<ProblemSummary[]>("GET", `/api/problems${q({ q: f.q, tags: f.tags?.join(","), min: f.min, max: f.max })}`),
  problem: (id: string) => call<ProblemDetail>("GET", `/api/problems/${encodeURIComponent(id)}`),
  saveSamples: (id: string, samples: { input: string; output: string }[]) =>
    call<ProblemDetail["samples"]>("PUT", `/api/problems/${encodeURIComponent(id)}/samples`, samples),
  contests: () => call<ContestListItem[]>("GET", "/api/contests"),
  contest: (id: string) => call<ContestDetail>("GET", `/api/contests/${encodeURIComponent(id)}`),
  createContest: (name: string) => call<ContestDetail>("POST", "/api/contests", { name }),
  llmSettings: () => call<LlmSettings>("GET", "/api/settings/llm"),
  saveLlmSettings: (settings: Pick<LlmSettings, "apiFormat" | "baseUrl" | "model" | "reasoningEffort"> & { apiKey?: string; clearApiKey?: boolean }) =>
    call<LlmSettings>("PUT", "/api/settings/llm", settings),
  llmModels: () => call<{ models: string[] }>("GET", "/api/llm/models"),

  setterEnv: () => call<SetterEnv>("GET", "/api/setter/env"),
  setterIdeas: (brief: Brief) => call<{ job: string }>("POST", "/api/setter/ideas", { brief }),
  setterWrite: (brief: Brief, idea: Idea) => call<{ job: string }>("POST", "/api/setter/write", { brief, idea }),
  setterRevise: (brief: Brief, draft: Draft, instruction: string) => call<{ job: string }>("POST", "/api/setter/revise", { brief, draft, instruction }),
  setterVerify: (brief: Brief, draft: Draft) => call<{ job: string }>("POST", "/api/setter/verify", { brief, draft }),
  setterCommit: (brief: Brief, draft: Draft, report: VerifyReport | null, contest: string | null) =>
    call<{ id: string }>("POST", "/api/setter/commit", { brief, draft, report, contest }),
  setterPageUrl: (page: number, scale: number, rev: number) => `/api/setter/page/${page}${q({ scale: scale.toFixed(2), rev })}`,
  job: <T>(id: string) => call<Job<T>>("GET", `/api/jobs/${id}`),
  search: (text: string) => call<FuzzyHit[]>("GET", `/api/search${q({ q: text })}`),

  read: (path: string) => call<{ text: string; hash: string }>("GET", `/api/file${q({ path })}`),
  write: (path: string, text: string, hash?: string) => call<{ hash: string }>("PUT", "/api/file", { path, text, hash }),
  asset: (path: string) => `/api/asset${q({ path })}`,

  compile: (d: DocRef, unsaved?: { path: string; text: string }) =>
    call<CompileResult>("POST", `/api/compile/${docPath(d)}${q({ editorial: d.editorial })}`, { unsaved }),
  pageUrl: (d: DocRef, page: number, scale: number, rev: number) =>
    `/api/page/${docPath(d)}/${page}${q({ editorial: d.editorial, scale: scale.toFixed(2), rev })}`,
  pdfUrl: (d: DocRef, download = false) => `/api/pdf/${docPath(d)}${q({ editorial: d.editorial, download })}`,
  locate: (d: DocRef, page: number, x: number, y: number) =>
    call<{ file: string; line: number; column: number } | null>("GET", `/api/locate/${docPath(d)}/${page}${q({ editorial: d.editorial, x, y })}`),
};

/** Server-sent workspace events: `changed` (paths edited outside the app) and `workspace` (switched). */
export function subscribe(handler: (type: string, data: { paths?: string[] }) => void): () => void {
  const es = new EventSource("/api/events");
  const on = (type: string) => (e: MessageEvent) => handler(type, e.data ? JSON.parse(e.data) : {});
  es.addEventListener("changed", on("changed"));
  es.addEventListener("workspace", on("workspace"));
  return () => es.close();
}
