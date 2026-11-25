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

export interface SearchQuery {
  text?: string;
  tags?: string[];
  ratingMin?: number;
  ratingMax?: number;
  platform?: string;
}

export interface ProblemMeta {
  title: string;
  rating?: number;
  tags?: string[];
  author?: string;
  limits: Limits;
  source?: Source[];
  [k: string]: unknown;
}

export interface ProblemDetail {
  id: string;
  dir: string;
  meta: ProblemMeta;
  statements: { language: "zh" | "en" | null; path: string; text: string }[];
  samples: { name: string; input: string; output: string }[];
  editorial: { path: string; text: string } | null;
}

export interface ContestItem { id: string; label: string; title?: string; limits?: Limits; [k: string]: unknown }

export interface ContestMeta {
  name: string;
  location?: string;
  date?: string;
  author?: string;
  language: "chinese" | "english";
  frozen: boolean;
  problem: ContestItem[];
  [k: string]: unknown;
}

export interface ContestListItem {
  id: string;
  meta: ContestMeta;
  stats: { total: number; [k: string]: unknown };
}

export interface ContestDetail {
  id: string;
  dir: string;
  meta: ContestMeta;
  items: { item: ContestItem; problem: { id: string; dir: string; title: string } | null }[];
}

export interface FuzzyHit { kind: "problem" | "contest"; id: string; title: string; [k: string]: unknown }

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
  /** Width and height of each page in typographic points. */
  pageSizes: [number, number][];
}

export interface OpenOptions { readOnly?: boolean; fontDirs?: string[] }
export interface SessionOptions { editorial?: boolean; language?: "zh" | "en" }

export declare class Session {
  /** Converts, lints and typesets, overlaying the editor's unsaved text. */
  compile(unsaved?: { path: string; text: string } | null): Promise<CompileReport>;
  /** Page (0-based) as PNG at `scale` pixels per point. Rejects with `[not_compiled]` before the first successful compile. */
  render(page: number, scale: number): Promise<Buffer>;
  pdf(): Promise<Buffer>;
  readonly defaultOutput: string;
  locate(page: number, x: number, y: number): { file: string; line: number; column: number } | null;
}

export declare class Workstation {
  static open(root: string, options?: OpenOptions): Workstation;
  static init(dir: string, template?: string): Workstation;
  static initExample(dir: string, language?: "zh" | "en"): Workstation;
  readonly root: string;
  readonly readOnly: boolean;
  refresh(rel?: string): void;
  problems(query?: SearchQuery): ProblemSummary[];
  contests(): ContestListItem[];
  fuzzy(query: string, limit?: number): FuzzyHit[];
  problem(id: string): ProblemDetail;
  contest(id: string): ContestDetail;
  read(rel: string): { text: string; hash: string };
  readBytes(rel: string): Buffer;
  /** Atomic write; with `expectedHash` rejects with `[conflict]` if the file changed since read. */
  write(rel: string, text: string, expectedHash?: string): string;
  saveSamples(id: string, samples: { input: string; output: string }[]): void;
  session(kind: "problem" | "contest", id: string, options?: SessionOptions): Session;
}

export declare function presets(): { id: string; name: string }[];
export declare function version(): string;
/** `[conflict] …` → `"conflict"`. */
export declare function errorCode(err: unknown): string | undefined;
