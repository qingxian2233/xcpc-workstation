//! Node.js bindings for the XCPC Workstation core.
//!
//! Only glue lives here, like `apps/desktop` and `xcpc-mcp`: storage, publishing and
//! typesetting are `xcpc-storage`'s, `xcpc-convert`'s and `xcpc-engine`'s. Everything that
//! takes more than a moment (compiling, rasterizing, PDF) runs on libuv's thread pool as an
//! [`AsyncTask`], so the Node event loop never blocks.
//!
//! Values cross as plain JSON (`serde_json::Value`); errors carry the storage error code in
//! brackets (`[conflict] …`) so callers can branch on it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json::{Value, json};
use xcpc_convert::Document;
use xcpc_convert::publish::{self, Kind, Report, Target};
use xcpc_domain::{Language, Problem, SearchQuery};
use xcpc_engine::{Compiled, Engine, FontOptions};
use xcpc_storage::{Workspace, WorkspaceIndex};

// ---- Errors ------------------------------------------------------------------------------

fn storage_err(e: xcpc_storage::Error) -> Error {
    Error::new(
        Status::GenericFailure,
        format!("[{}] {}", e.code(), e.human_message()),
    )
}

fn plain_err(code: &str, msg: impl std::fmt::Display) -> Error {
    Error::new(Status::GenericFailure, format!("[{code}] {msg}"))
}

fn to_json<T: serde::Serialize>(v: &T) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| plain_err("internal", e))
}

fn parse_language(s: Option<&str>) -> Option<Language> {
    match s?.to_ascii_lowercase().as_str() {
        "zh" | "chinese" => Some(Language::Chinese),
        "en" | "english" => Some(Language::English),
        _ => None,
    }
}

fn language_str(l: Option<Language>) -> Option<&'static str> {
    l.map(|l| match l {
        Language::Chinese => "zh",
        Language::English => "en",
    })
}

/// Fonts: `XCPC_FONT_DIRS` (or `fontDirs`) first, then the system's so CJK always resolves.
fn font_options(extra: Option<Vec<String>>) -> FontOptions {
    let mut fonts = FontOptions::from_env();
    if let Some(dirs) = extra {
        fonts.dirs.extend(dirs.into_iter().map(PathBuf::from));
    }
    fonts.system = true;
    fonts
}

// ---- Workstation -------------------------------------------------------------------------

struct Inner {
    ws: Workspace,
    index: WorkspaceIndex,
}

/// An open workspace: the problem library, contests and their files.
#[napi]
pub struct Workstation {
    inner: Arc<Mutex<Inner>>,
    fonts: FontOptions,
}

impl Workstation {
    fn from_ws(ws: Workspace, font_dirs: Option<Vec<String>>) -> Result<Self> {
        let index = WorkspaceIndex::scan(&ws).map_err(storage_err)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner { ws, index })),
            fonts: font_options(font_dirs),
        })
    }

    fn ws(&self) -> Workspace {
        self.inner.lock().unwrap().ws.clone()
    }
}

#[napi(object)]
pub struct OpenOptions {
    pub read_only: Option<bool>,
    /// Extra font directories, scanned after `XCPC_FONT_DIRS`.
    pub font_dirs: Option<Vec<String>>,
}

#[napi]
impl Workstation {
    /// Opens the workspace at `root` (the directory holding `workspace.toml`).
    #[napi(factory)]
    pub fn open(root: String, options: Option<OpenOptions>) -> Result<Self> {
        let (ro, dirs) = options.map_or((false, None), |o| {
            (o.read_only.unwrap_or(false), o.font_dirs)
        });
        let ws = Workspace::open(&root, ro).map_err(storage_err)?;
        Self::from_ws(ws, dirs)
    }

    /// Creates an empty workspace at `dir` and opens it.
    #[napi(factory)]
    pub fn init(dir: String, template: Option<String>) -> Result<Self> {
        let ws = xcpc_storage::init_workspace_with_template(&dir, template.as_deref())
            .map_err(storage_err)?;
        Self::from_ws(ws, None)
    }

    /// Writes the built-in example workspace (3 problems, 2 contests) to `dir` and opens it.
    #[napi(factory)]
    pub fn init_example(dir: String, language: Option<String>) -> Result<Self> {
        let lang = parse_language(language.as_deref()).unwrap_or(Language::Chinese);
        let ws = xcpc_storage::init_example_workspace_with_options(&dir, None, lang)
            .map_err(storage_err)?;
        Self::from_ws(ws, None)
    }

    #[napi(getter)]
    pub fn root(&self) -> String {
        self.ws().root().to_string_lossy().into_owned()
    }

    #[napi(getter)]
    pub fn read_only(&self) -> bool {
        self.ws().is_read_only()
    }

    /// Rescans the index after files changed (`rel` limits it to one path).
    #[napi]
    pub fn refresh(&self, rel: Option<String>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let ws = inner.ws.clone();
        match rel {
            Some(rel) => inner.index.refresh_path(&ws, &rel).map_err(storage_err),
            None => {
                inner.index = WorkspaceIndex::scan(&ws).map_err(storage_err)?;
                Ok(())
            }
        }
    }

    /// Problem summaries, filtered by `{ text, tags, ratingMin, ratingMax, platform }`.
    #[napi(ts_return_type = "ProblemSummary[]")]
    pub fn problems(
        &self,
        #[napi(ts_arg_type = "SearchQuery")] query: Option<Value>,
    ) -> Result<Value> {
        let q: SearchQuery = match query {
            Some(v) => {
                serde_json::from_value(camel_to_snake(v)).map_err(|e| plain_err("invalid", e))?
            }
            None => SearchQuery::default(),
        };
        let inner = self.inner.lock().unwrap();
        to_json(&inner.index.search_problems(&q))
    }

    /// Contests with their metadata and difficulty breakdown.
    #[napi(ts_return_type = "ContestListItem[]")]
    pub fn contests(&self) -> Result<Value> {
        let inner = self.inner.lock().unwrap();
        let list: Vec<Value> = inner
            .index
            .contests()
            .into_iter()
            .map(|(id, meta)| {
                json!({
                    "id": id,
                    "meta": meta,
                    "stats": inner.index.contest_stats(meta),
                })
            })
            .collect();
        Ok(Value::Array(list))
    }

    /// Quick-open search across problems and contests.
    #[napi(ts_return_type = "FuzzyHit[]")]
    pub fn fuzzy(&self, query: String, limit: Option<u32>) -> Result<Value> {
        let inner = self.inner.lock().unwrap();
        to_json(
            &inner
                .index
                .fuzzy_search(&query, limit.unwrap_or(20) as usize),
        )
    }

    /// A problem with its statements, samples and editorial.
    #[napi(ts_return_type = "ProblemDetail")]
    pub fn problem(&self, id: String) -> Result<Value> {
        let p = self.ws().load_problem(&id).map_err(storage_err)?;
        problem_json(&p)
    }

    /// A contest with its items resolved to the problems they reference.
    #[napi(ts_return_type = "ContestDetail")]
    pub fn contest(&self, id: String) -> Result<Value> {
        let ws = self.ws();
        let c = ws.load_contest(&id).map_err(storage_err)?;
        let resolved = ws.contest_problems(&c).map_err(storage_err)?;
        let items: Vec<Value> = c
            .meta
            .problems
            .iter()
            .map(|item| {
                let problem = resolved.iter().find(|(i, _)| i.id == item.id).map(|(_, p)| {
                    json!({ "id": p.id, "dir": p.dir, "title": item.apply(&p.meta).title })
                });
                json!({ "item": item, "problem": problem })
            })
            .collect();
        Ok(json!({ "id": c.id, "dir": c.dir, "meta": c.meta, "items": items }))
    }

    /// A workspace text file with its SHA-256 (for conflict-checked saves).
    #[napi(ts_return_type = "{ text: string; hash: string }")]
    pub fn read(&self, rel: String) -> Result<Value> {
        let (text, hash) = self.ws().read_text_with_hash(&rel).map_err(storage_err)?;
        Ok(json!({ "text": text, "hash": hash }))
    }

    /// A workspace file's bytes (pictures).
    #[napi]
    pub fn read_bytes(&self, rel: String) -> Result<Buffer> {
        Ok(self.ws().read_bytes(&rel).map_err(storage_err)?.into())
    }

    /// Atomically writes `text` to `rel` and returns its new hash. With `expectedHash`,
    /// fails with `[conflict]` if someone else changed the file since it was read
    /// (`""` expects it not to exist).
    #[napi]
    pub fn write(
        &self,
        rel: String,
        text: String,
        expected_hash: Option<String>,
    ) -> Result<String> {
        let mut inner = self.inner.lock().unwrap();
        let ws = inner.ws.clone();
        let hash = ws
            .write_checked(&rel, text.as_bytes(), expected_hash.as_deref())
            .map_err(storage_err)?;
        let _ = inner.index.refresh_path(&ws, &rel);
        Ok(hash)
    }

    /// Replaces a problem's samples (`samples/NN.in` / `.ans`), renumbering contiguously.
    #[napi]
    pub fn save_samples(
        &self,
        id: String,
        #[napi(ts_arg_type = "{ input: string; output: string }[]")] samples: Value,
    ) -> Result<()> {
        #[derive(serde::Deserialize)]
        struct Pair {
            input: String,
            output: String,
        }
        let pairs: Vec<Pair> =
            serde_json::from_value(samples).map_err(|e| plain_err("invalid", e))?;
        let pairs: Vec<(String, String)> = pairs.into_iter().map(|p| (p.input, p.output)).collect();
        let mut inner = self.inner.lock().unwrap();
        let ws = inner.ws.clone();
        ws.save_samples(&id, &pairs).map_err(storage_err)?;
        let _ = inner.index.refresh_problem(&ws, &id);
        Ok(())
    }

    /// A typesetting session for one problem or contest. Each keeps its own Typst world, so
    /// recompiling after an edit is incremental.
    #[napi]
    pub fn session(
        &self,
        kind: String,
        id: String,
        options: Option<SessionOptions>,
    ) -> Result<Session> {
        let target = match kind.as_str() {
            "problem" => Target::Problem(id),
            "contest" => Target::Contest(id),
            _ => {
                return Err(plain_err(
                    "invalid",
                    format!("kind must be problem or contest, not {kind}"),
                ));
            }
        };
        let opts = options.unwrap_or(SessionOptions {
            editorial: None,
            language: None,
        });
        let book = if opts.editorial.unwrap_or(false) {
            Kind::Editorial
        } else {
            Kind::Statements
        };
        let ws = self.ws();
        let engine = publish::engine(&ws, &self.fonts);
        Ok(Session {
            shared: Arc::new(SessionShared {
                ws,
                target,
                kind: book,
                language: parse_language(opts.language.as_deref()),
                engine: Mutex::new(engine),
                last: Mutex::new(None),
            }),
        })
    }
}

fn problem_json(p: &Problem) -> Result<Value> {
    let statements: Vec<Value> = p
        .statements
        .iter()
        .map(|s| json!({ "language": language_str(s.language), "path": s.file.path, "text": s.file.text }))
        .collect();
    Ok(json!({
        "id": p.id,
        "dir": p.dir,
        "meta": p.meta,
        "statements": statements,
        "samples": p.samples,
        "editorial": p.editorial.as_ref().map(|e| json!({ "path": e.path, "text": e.text })),
    }))
}

/// `ratingMin` → `rating_min` for the domain's snake_case serde names.
fn camel_to_snake(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    let mut s = String::new();
                    for c in k.chars() {
                        if c.is_ascii_uppercase() {
                            s.push('_');
                            s.push(c.to_ascii_lowercase());
                        } else {
                            s.push(c);
                        }
                    }
                    (s, v)
                })
                .collect(),
        ),
        v => v,
    }
}

// ---- Session -----------------------------------------------------------------------------

#[napi(object)]
pub struct SessionOptions {
    /// Typeset the editorial book instead of the statements.
    pub editorial: Option<bool>,
    /// `zh` or `en`: a single problem's statement language.
    pub language: Option<String>,
}

#[napi(object)]
pub struct Unsaved {
    /// Workspace-relative path of the file being edited (`problems/a/statement.md`).
    pub path: String,
    pub text: String,
}

struct Last {
    compiled: Compiled,
    doc: Document,
}

struct SessionShared {
    ws: Workspace,
    target: Target,
    kind: Kind,
    language: Option<Language>,
    engine: Mutex<Engine>,
    /// The latest successful compilation; rendering reads it without waiting on `engine`.
    last: Mutex<Option<Arc<Last>>>,
}

/// Typesets one problem or contest; keeps the latest pages for rendering.
#[napi]
pub struct Session {
    shared: Arc<SessionShared>,
}

#[napi]
impl Session {
    /// Converts, lints and typesets, overlaying `unsaved` (the editor's text) on disk.
    /// Resolves with the report; on success the pages become available to `render`.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn compile(&self, unsaved: Option<Unsaved>) -> AsyncTask<CompileTask> {
        AsyncTask::new(CompileTask {
            shared: self.shared.clone(),
            unsaved: unsaved.map(|u| (u.path, u.text)),
        })
    }

    /// Page `page` (0-based) as PNG at `scale` pixels per point.
    #[napi(ts_return_type = "Promise<Buffer>")]
    pub fn render(&self, page: u32, scale: f64) -> Result<AsyncTask<RenderTask>> {
        let last = self.last()?;
        Ok(AsyncTask::new(RenderTask {
            last,
            page: page as usize,
            scale: scale as f32,
        }))
    }

    /// The whole document as PDF.
    #[napi(ts_return_type = "Promise<Buffer>")]
    pub fn pdf(&self) -> Result<AsyncTask<PdfTask>> {
        Ok(AsyncTask::new(PdfTask { last: self.last()? }))
    }

    /// Where the PDF goes by default (`build/<name>.pdf`).
    #[napi(getter)]
    pub fn default_output(&self) -> String {
        publish::default_output(&self.shared.ws, &self.shared.target, self.shared.kind)
    }

    /// Maps a point (typographic points from the top-left of 0-based `page`) back to the
    /// Markdown source.
    #[napi(ts_return_type = "{ file: string; line: number; column: number } | null")]
    pub fn locate(&self, page: u32, x: f64, y: f64) -> Result<Value> {
        let last = self.last()?;
        let engine = self.shared.engine.lock().unwrap();
        let loc = publish::locate_point(&engine, &last.compiled, &last.doc, page as usize, x, y);
        to_json(&loc)
    }

    fn last(&self) -> Result<Arc<Last>> {
        self.shared
            .last
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| plain_err("not_compiled", "nothing compiled yet"))
    }
}

pub struct CompileTask {
    shared: Arc<SessionShared>,
    unsaved: Option<(String, String)>,
}

impl Task for CompileTask {
    type Output = (Report, Vec<(f64, f64)>);
    type JsValue = String;

    fn compute(&mut self) -> Result<Self::Output> {
        let s = &self.shared;
        let mut engine = s.engine.lock().unwrap();
        let unsaved = self.unsaved.as_ref().map(|(p, t)| (p.as_str(), t.as_str()));
        let (compiled, doc, report) =
            publish::check(&s.ws, &mut engine, &s.target, s.kind, s.language, unsaved)
                .map_err(storage_err)?;
        let mut sizes = Vec::new();
        if let (Some(compiled), Some(doc)) = (compiled, doc) {
            sizes = (0..compiled.page_count())
                .filter_map(|i| compiled.page_size(i))
                .collect();
            *s.last.lock().unwrap() = Some(Arc::new(Last { compiled, doc }));
        }
        Ok((report, sizes))
    }

    /// JSON text (`Value` is not a task result type); `@xcpc/core` parses it.
    fn resolve(&mut self, _env: Env, (report, sizes): Self::Output) -> Result<String> {
        let mut v = to_json(&report)?;
        v["pageSizes"] = json!(sizes.iter().map(|(w, h)| [w, h]).collect::<Vec<_>>());
        Ok(v.to_string())
    }
}

pub struct RenderTask {
    last: Arc<Last>,
    page: usize,
    scale: f32,
}

impl Task for RenderTask {
    type Output = Vec<u8>;
    type JsValue = Buffer;

    fn compute(&mut self) -> Result<Vec<u8>> {
        self.last
            .compiled
            .render_png(self.page, self.scale.clamp(0.1, 8.0))
            .ok_or_else(|| plain_err("not_found", format!("no page {}", self.page)))
    }

    fn resolve(&mut self, _env: Env, out: Vec<u8>) -> Result<Buffer> {
        Ok(out.into())
    }
}

pub struct PdfTask {
    last: Arc<Last>,
}

impl Task for PdfTask {
    type Output = Vec<u8>;
    type JsValue = Buffer;

    fn compute(&mut self) -> Result<Vec<u8>> {
        self.last.compiled.pdf().map_err(|diags| {
            let msg = diags
                .iter()
                .map(|d| d.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            plain_err("pdf", msg)
        })
    }

    fn resolve(&mut self, _env: Env, out: Vec<u8>) -> Result<Buffer> {
        Ok(out.into())
    }
}

/// The preset templates bundled with the engine.
#[napi(ts_return_type = "{ id: string; name: string }[]")]
pub fn presets() -> Result<Value> {
    to_json(&xcpc_engine::list_presets())
}

/// Core version.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
