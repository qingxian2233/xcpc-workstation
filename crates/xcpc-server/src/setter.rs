//! `/api/setter/*`: the problem setter over HTTP. Model calls and verification take from
//! seconds to minutes, so each runs as a background job the UI polls (`/api/jobs/<id>`).
//! A draft is previewed by writing it into a scratch workspace that shares the real one's
//! templates, so the pages look exactly like the saved problem will.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Value, json};
use xcpc_runtime::{Error, Session, Workstation};
use xcpc_setter::pipeline::{self, Progress, Start};
use xcpc_setter::{Brief, Draft, Idea, Report, judge};

use crate::{AppState, err};

fn setter_err(e: xcpc_setter::Error) -> Error {
    err(&e.code, e.message)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub done: bool,
    pub stage: String,
    pub round: usize,
    pub report: Option<Report>,
    pub result: Option<Value>,
    pub error: Option<Error>,
}

struct Preview {
    session: Arc<Session>,
    // Keeps the scratch workspace's index alive with its session.
    _station: Workstation,
}

#[derive(Clone, Default)]
pub struct SetterState {
    jobs: Arc<Mutex<HashMap<String, Arc<Mutex<Job>>>>>,
    preview: Arc<Mutex<Option<Preview>>>,
}

fn parse<T: serde::de::DeserializeOwned>(v: &Value, key: &str) -> Result<T, Error> {
    serde_json::from_value(v.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| err("invalid", format!("{key}: {e}")))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir(&entry.path(), &to.join(entry.file_name()))?;
        } else if ty.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

impl AppState {
    fn spawn_job(
        &self,
        work: impl FnOnce(&dyn Fn(Progress)) -> Result<Value, Error> + Send + 'static,
    ) -> Value {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let job = Arc::new(Mutex::new(Job::default()));
        {
            let mut jobs = self.setter.jobs.lock().unwrap();
            // Forget finished jobs nobody asked about for a while.
            if jobs.len() > 32 {
                jobs.retain(|_, j| !j.lock().unwrap().done);
            }
            jobs.insert(id.clone(), job.clone());
        }
        std::thread::spawn(move || {
            let progress = |p: Progress| {
                let mut j = job.lock().unwrap();
                j.stage = p.stage;
                j.round = p.round;
                if p.report.is_some() {
                    j.report = p.report;
                }
            };
            let outcome =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&progress)))
                    .unwrap_or_else(|_| Err(err("internal", "出题任务意外中断")));
            let mut j = job.lock().unwrap();
            match outcome {
                Ok(v) => j.result = Some(v),
                Err(e) => j.error = Some(e),
            }
            j.done = true;
        });
        json!({ "job": id })
    }

    fn library_examples(&self, brief: &Brief) -> Vec<String> {
        let library: Vec<xcpc_domain::ProblemSummary> = self
            .with_ws(|w| w.problems(None))
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        xcpc_setter::calibration(&library, brief)
    }

    /// Typesets `draft` in a scratch copy of the workspace's template setup.
    fn preview_draft(
        &self,
        brief: &Brief,
        draft: &Draft,
        report: Option<&Report>,
    ) -> Result<Value, Error> {
        let real = self.with_ws(|w| Ok(w.workspace()))?;
        let dir: PathBuf = std::env::temp_dir().join(format!("xcpc-draft-{}", &self.token[..8]));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("problems")).map_err(|e| err("io", e))?;
        std::fs::create_dir_all(dir.join("contests")).map_err(|e| err("io", e))?;
        let root = real.root();
        std::fs::copy(root.join("workspace.toml"), dir.join("workspace.toml"))
            .map_err(|e| err("io", e))?;
        if root.join("templates").is_dir() {
            copy_dir(&root.join("templates"), &dir.join("templates")).map_err(|e| err("io", e))?;
        }
        let scratch = xcpc_storage::Workspace::open(&dir, false)
            .map_err(|e| err(e.code(), e.human_message()))?;
        let id = xcpc_setter::commit(&scratch, brief, draft, report, None).map_err(setter_err)?;
        let mut station = Workstation::open(dir.to_string_lossy().into_owned(), None)?;
        station.set_font_dirs(self.fonts.clone());
        let session = Arc::new(station.session("problem".into(), id, None)?);
        let seq = session.next_sequence();
        let compiled = session.compile(None, seq)?;
        *self.setter.preview.lock().unwrap() = Some(Preview {
            session,
            _station: station,
        });
        Ok(compiled)
    }

    /// `/api/setter/*` and `/api/jobs/<id>`.
    pub(crate) fn setter_route(
        &self,
        method: &str,
        p: &[&str],
        q: &HashMap<String, String>,
        value: &Value,
    ) -> Result<Response, Error> {
        let result = match (method, p) {
            ("GET", ["api", "jobs", id]) => {
                let job = self
                    .setter
                    .jobs
                    .lock()
                    .unwrap()
                    .get(*id)
                    .cloned()
                    .ok_or_else(|| err("not_found", "任务不存在"))?;
                let snapshot = job.lock().unwrap().clone();
                serde_json::to_value(snapshot).map_err(|e| err("internal", e))?
            }
            ("GET", ["api", "setter", "env"]) => {
                let compiler = match judge::detect() {
                    Ok(tc) => json!({"ok": true, "version": tc.version}),
                    Err(e) => json!({"ok": false, "message": e.message}),
                };
                let s = self.current_llm_settings();
                json!({
                    "compiler": compiler,
                    "llm": {"configured": !s.base_url.is_empty() && !s.model.is_empty(), "model": s.model},
                    "tiers": xcpc_setter::prompts::tiers(),
                })
            }
            ("POST", ["api", "setter", "ideas"]) => {
                let brief: Brief = parse(value, "brief")?;
                let llm = self.llm()?;
                let examples = self.library_examples(&brief);
                self.spawn_job(move |progress| {
                    progress(Progress {
                        stage: "构思方案".into(),
                        round: 0,
                        report: None,
                    });
                    let ideas = xcpc_setter::ideas(&llm, &brief, &examples).map_err(setter_err)?;
                    Ok(json!({ "ideas": ideas }))
                })
            }
            ("POST", ["api", "setter", action @ ("write" | "revise" | "verify")]) => {
                let brief: Brief = parse(value, "brief")?;
                let start = match *action {
                    "write" => Start::Write(parse::<Idea>(value, "idea")?),
                    "revise" => {
                        let instruction: String = parse(value, "instruction")?;
                        if instruction.trim().is_empty() {
                            return Err(err("invalid", "请写下要怎么改"));
                        }
                        Start::Revise(parse::<Draft>(value, "draft")?, instruction)
                    }
                    _ => Start::Verify(parse::<Draft>(value, "draft")?),
                };
                // Hand edits can be verified without a model.
                let llm = if *action == "verify" {
                    self.llm().unwrap_or_default()
                } else {
                    self.llm()?
                };
                let toolchain = judge::detect().ok();
                let state = self.clone();
                self.spawn_job(move |progress| {
                    let out = pipeline::produce(&llm, &brief, toolchain.as_ref(), start, progress)
                        .map_err(setter_err)?;
                    progress(Progress {
                        stage: "排版预览".into(),
                        round: out.rounds,
                        report: out.report.clone(),
                    });
                    let preview = state
                        .preview_draft(&brief, &out.draft, out.report.as_ref())
                        .unwrap_or_else(|e| json!({"error": e}));
                    Ok(json!({
                        "draft": out.draft,
                        "report": out.report,
                        "rounds": out.rounds,
                        "preview": preview,
                        "statement": xcpc_setter::statement(&out.draft),
                    }))
                })
            }
            ("GET", ["api", "setter", "page", page]) => {
                let session = self
                    .setter
                    .preview
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|p| p.session.clone())
                    .ok_or_else(|| err("not_compiled", "没有草稿预览"))?;
                let page = page
                    .parse::<u32>()
                    .map_err(|_| err("invalid", "invalid page"))?;
                let scale = q
                    .get("scale")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|s| s.is_finite())
                    .unwrap_or(2.0);
                let rev = q.get("rev").and_then(|s| s.parse::<u64>().ok());
                let png = session.render(page, scale, rev)?;
                return Ok((
                    [("content-type", "image/png"), ("cache-control", "no-store")],
                    png,
                )
                    .into_response());
            }
            ("POST", ["api", "setter", "commit"]) => {
                let brief: Brief = parse(value, "brief")?;
                let draft: Draft = parse(value, "draft")?;
                let report: Option<Report> = parse(value, "report").ok().flatten();
                let contest: Option<String> = parse(value, "contest").ok().flatten();
                let ws = self.with_ws(|w| Ok(w.workspace()))?;
                let id =
                    xcpc_setter::commit(&ws, &brief, &draft, report.as_ref(), contest.as_deref())
                        .map_err(setter_err)?;
                self.with_ws(|w| w.refresh(None))?;
                json!({ "id": id })
            }
            _ => return Err(err("not_found", "unknown API route")),
        };
        Ok(axum::Json(result).into_response())
    }
}
