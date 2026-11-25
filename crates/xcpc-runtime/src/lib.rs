//! Native transport adapter shared by the Tauri shell and its HTTP compatibility layer.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use xcpc_convert::Document;
use xcpc_convert::publish::{self, Kind, Target};
use xcpc_domain::{ContestMetaPatch, Language, Problem, SearchQuery};
use xcpc_engine::{Compiled, Engine, FontOptions};
use xcpc_storage::{Workspace, WorkspaceIndex};

// ---- Errors ------------------------------------------------------------------------------

fn storage_err(e: xcpc_storage::Error) -> Error {
    Error {
        code: e.code().to_string(),
        message: e.human_message(),
    }
}

fn plain_err(code: &str, msg: impl std::fmt::Display) -> Error {
    Error {
        code: code.to_string(),
        message: msg.to_string(),
    }
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

pub struct OpenOptions {
    pub read_only: Option<bool>,
    /// Extra font directories, scanned after `XCPC_FONT_DIRS`.
    pub font_dirs: Option<Vec<String>>,
}

impl Workstation {
    /// Opens the workspace at `root` (the directory holding `workspace.toml`).
    pub fn open(root: String, options: Option<OpenOptions>) -> Result<Self> {
        let (ro, dirs) = options.map_or((false, None), |o| {
            (o.read_only.unwrap_or(false), o.font_dirs)
        });
        let ws = Workspace::open(&root, ro).map_err(storage_err)?;
        Self::from_ws(ws, dirs)
    }

    /// Creates an empty workspace at `dir` and opens it.
    pub fn init(dir: String, template: Option<String>) -> Result<Self> {
        let ws = xcpc_storage::init_workspace_with_template(&dir, template.as_deref())
            .map_err(storage_err)?;
        Self::from_ws(ws, None)
    }

    /// Writes the built-in example workspace (3 problems, 2 contests) to `dir` and opens it.
    pub fn init_example(dir: String, language: Option<String>) -> Result<Self> {
        let lang = parse_language(language.as_deref()).unwrap_or(Language::Chinese);
        let ws = xcpc_storage::init_example_workspace_with_options(&dir, None, lang)
            .map_err(storage_err)?;
        Self::from_ws(ws, None)
    }

    /// The storage handle, for core crates that work on the workspace directly.
    pub fn workspace(&self) -> Workspace {
        self.ws()
    }

    pub fn root(&self) -> String {
        self.ws().root().to_string_lossy().into_owned()
    }

    pub fn read_only(&self) -> bool {
        self.ws().is_read_only()
    }

    /// Rescans the index after files changed (`rel` limits it to one path).
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
    pub fn problems(&self, query: Option<Value>) -> Result<Value> {
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
    pub fn fuzzy(&self, query: String, limit: Option<u32>) -> Result<Value> {
        let inner = self.inner.lock().unwrap();
        to_json(
            &inner
                .index
                .fuzzy_search(&query, limit.unwrap_or(20) as usize),
        )
    }

    /// A problem with its statements, samples and editorial.
    pub fn problem(&self, id: String) -> Result<Value> {
        let p = self.ws().load_problem(&id).map_err(storage_err)?;
        problem_json(&p)
    }

    /// A contest with its items resolved to the problems they reference.
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

    /// Creates an empty contest for the composition workflow.
    pub fn create_contest(&self, id: String, name: String) -> Result<Value> {
        if self.read_only() {
            return Err(plain_err("read_only", "workspace is read-only"));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(plain_err("invalid", "contest name cannot be empty"));
        }
        self.ws()
            .compose_contest(
                &id,
                &[],
                Some(&ContestMetaPatch {
                    name: Some(name.to_owned()),
                    ..Default::default()
                }),
            )
            .map_err(storage_err)?;
        self.refresh(Some(format!("contests/{id}/contest.toml")))?;
        self.contest(id)
    }

    /// Saves a reviewed generated problem and appends it to an unfrozen contest.
    pub fn add_generated_problem(
        &self,
        contest_id: String,
        problem_id: String,
        draft: Value,
    ) -> Result<Value> {
        #[derive(serde::Deserialize)]
        struct SampleDraft {
            input: String,
            output: String,
        }
        #[derive(serde::Deserialize)]
        struct ProblemDraft {
            title: String,
            #[serde(default)]
            tags: Vec<String>,
            #[serde(default)]
            background: String,
            description: String,
            input: String,
            output: String,
            #[serde(default)]
            constraints: String,
            #[serde(default)]
            samples: Vec<SampleDraft>,
            #[serde(default)]
            editorial: String,
        }

        if self.read_only() {
            return Err(plain_err("read_only", "workspace is read-only"));
        }
        let draft: ProblemDraft =
            serde_json::from_value(draft).map_err(|e| plain_err("invalid", e))?;
        if draft.title.trim().is_empty()
            || draft.description.trim().is_empty()
            || draft.input.trim().is_empty()
            || draft.output.trim().is_empty()
            || draft.samples.len() > 12
        {
            return Err(plain_err(
                "invalid",
                "draft needs a title, statement sections, and at most 12 samples",
            ));
        }

        let ws = self.ws();
        let contest = ws.load_contest(&contest_id).map_err(storage_err)?;
        if contest.meta.frozen {
            return Err(plain_err(
                "contest_frozen",
                "cannot add to a frozen contest",
            ));
        }

        let mut problem = ws
            .create_problem(&problem_id, draft.title.trim())
            .map_err(storage_err)?;
        problem.meta.tags = draft
            .tags
            .into_iter()
            .map(|tag| tag.trim().to_owned())
            .filter(|tag| !tag.is_empty())
            .collect();

        let mut statement = String::new();
        if !draft.background.trim().is_empty() {
            statement.push_str("## 题目背景\n\n");
            statement.push_str(draft.background.trim());
            statement.push_str("\n\n");
        }
        statement.push_str("## 题目描述\n\n");
        statement.push_str(draft.description.trim());
        if !draft.constraints.trim().is_empty() {
            statement.push_str("\n\n## 数据范围\n\n");
            statement.push_str(draft.constraints.trim());
        }
        statement.push_str("\n\n## 输入格式\n\n");
        statement.push_str(draft.input.trim());
        statement.push_str("\n\n## 输出格式\n\n");
        statement.push_str(draft.output.trim());
        if !draft.samples.is_empty() {
            statement.push_str("\n\n## 样例\n");
        }

        let mut files = std::collections::BTreeMap::new();
        files.insert(
            "problem.toml".to_owned(),
            toml::to_string(&problem.meta).map_err(|e| plain_err("internal", e))?,
        );
        files.insert("statement.md".to_owned(), statement);
        if !draft.editorial.trim().is_empty() {
            files.insert("editorial.md".to_owned(), draft.editorial.trim().to_owned());
        }
        ws.write_problem_files(&problem_id, &files, None)
            .map_err(storage_err)?;

        let samples = draft
            .samples
            .into_iter()
            .map(|sample| (sample.input, sample.output))
            .collect::<Vec<_>>();
        if !samples.is_empty() {
            ws.save_samples(&problem_id, &samples)
                .map_err(storage_err)?;
        }
        ws.add_problem_to_contest(&contest_id, &problem_id)
            .map_err(storage_err)?;

        self.refresh(Some(format!("problems/{problem_id}/problem.toml")))?;
        self.refresh(Some(format!("contests/{contest_id}/contest.toml")))?;
        let problem = self.problem(problem_id)?;
        let contest = self.contest(contest_id)?;
        Ok(json!({ "problem": problem, "contest": contest }))
    }

    /// A workspace text file with its SHA-256 (for conflict-checked saves).
    pub fn read(&self, rel: String) -> Result<Value> {
        let (text, hash) = self.ws().read_text_with_hash(&rel).map_err(storage_err)?;
        Ok(json!({ "text": text, "hash": hash }))
    }

    /// A workspace file's bytes (pictures).
    pub fn read_bytes(&self, rel: String) -> Result<Vec<u8>> {
        Ok(self.ws().read_bytes(&rel).map_err(storage_err)?)
    }

    /// Atomically writes `text` to `rel` and returns its new hash. With `expectedHash`,
    /// fails with `[conflict]` if someone else changed the file since it was read
    /// (`""` expects it not to exist).
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
    pub fn save_samples(&self, id: String, samples: Value) -> Result<()> {
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
                fonts: self.fonts.clone(),
                engine: Mutex::new(engine),
                last: Mutex::new(None),
                sequence: std::sync::atomic::AtomicU64::new(0),
                revision: std::sync::atomic::AtomicU64::new(0),
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl Workstation {
    pub fn set_font_dirs(&mut self, dirs: Vec<String>) {
        self.fonts.dirs.extend(dirs.into_iter().map(PathBuf::from));
    }
}
pub struct SessionOptions {
    pub editorial: Option<bool>,
    pub language: Option<String>,
}
#[derive(serde::Deserialize)]
pub struct Unsaved {
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
    fonts: FontOptions,
    engine: Mutex<Engine>,
    last: Mutex<Option<Arc<Last>>>,
    sequence: std::sync::atomic::AtomicU64,
    revision: std::sync::atomic::AtomicU64,
}
static NEXT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub struct Session {
    shared: Arc<SessionShared>,
}
impl Session {
    pub fn next_sequence(&self) -> u64 {
        self.shared
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }
    pub fn compile(&self, unsaved: Option<Unsaved>, sequence: u64) -> Result<Value> {
        use std::sync::atomic::Ordering::SeqCst;
        let s = &self.shared;
        let mut engine = s.engine.lock().unwrap();
        if sequence != s.sequence.load(SeqCst) {
            return Ok(json!({"superseded": true}));
        }
        let overlay = unsaved.as_ref().map(|u| (u.path.as_str(), u.text.as_str()));
        let (compiled, doc, report) =
            publish::check(&s.ws, &mut engine, &s.target, s.kind, s.language, overlay)
                .map_err(storage_err)?;
        let mut sizes = Vec::new();
        if let (Some(compiled), Some(doc)) = (compiled, doc) {
            sizes = (0..compiled.page_count())
                .filter_map(|i| compiled.page_size(i))
                .collect();
            *s.last.lock().unwrap() = Some(Arc::new(Last { compiled, doc }));
            s.revision
                .store(NEXT_REVISION.fetch_add(1, SeqCst) + 1, SeqCst);
        }
        let mut v = to_json(&report)?;
        v["pageSizes"] = json!(sizes);
        v["rev"] = json!(s.revision.load(SeqCst));
        Ok(v)
    }
    fn last(&self) -> Result<Arc<Last>> {
        self.shared
            .last
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| plain_err("not_compiled", "nothing compiled yet"))
    }
    pub fn render(&self, page: u32, scale: f64, revision: Option<u64>) -> Result<Vec<u8>> {
        let s = &self.shared;
        // Read the snapshot and revision under the engine lock, avoiding stale pages
        // being cached under a revision they do not belong to.
        let _engine = s.engine.lock().unwrap();
        if let Some(rev) = revision {
            if rev != s.revision.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(plain_err("conflict", "preview has changed"));
            }
        }
        self.last()?
            .compiled
            .render_png(page as usize, scale.clamp(0.25, 6.0) as f32)
            .ok_or_else(|| plain_err("not_found", format!("no page {page}")))
    }
    pub fn export_pdf(&self) -> Result<Vec<u8>> {
        // A separate disk-only compilation cannot overwrite the editor's unsaved preview.
        let s = &self.shared;
        let mut engine = publish::engine(&s.ws, &s.fonts);
        let (compiled, _, report) =
            publish::check(&s.ws, &mut engine, &s.target, s.kind, s.language, None)
                .map_err(storage_err)?;
        let compiled = compiled.ok_or_else(|| {
            plain_err(
                "compile_failed",
                report
                    .diagnostics
                    .iter()
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        compiled.pdf().map_err(|ds| {
            plain_err(
                "pdf",
                ds.iter()
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }
    pub fn default_output(&self) -> String {
        publish::default_output(&self.shared.ws, &self.shared.target, self.shared.kind)
    }
    pub fn locate(&self, page: u32, x: f64, y: f64) -> Result<Value> {
        let engine = self.shared.engine.lock().unwrap();
        let Ok(last) = self.last() else {
            return Ok(Value::Null);
        };
        to_json(&publish::locate_point(
            &engine,
            &last.compiled,
            &last.doc,
            page as usize,
            x,
            y,
        ))
    }
}
