//! The app's local HTTP API, served in-process on a random loopback port with a
//! per-launch token. The Tauri shell only opens a window on it; `xcpc-serve` runs it
//! headless for tests and development.

mod secrets;
mod setter;
use axum::{
    Router,
    body::Bytes,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{
        IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{any, get},
};
use futures_util::StreamExt;
use notify::{RecursiveMode, Watcher};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
use xcpc_runtime::{Error, OpenOptions, Session, SessionOptions, Unsaved, Workstation};

#[derive(rust_embed::RustEmbed)]
#[folder = "../../apps/web/dist/"]
struct Web;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    /// `chat`, `responses` or `messages`; empty (older preferences) means `chat`.
    #[serde(default)]
    pub api_format: String,
    pub base_url: String,
    pub model: String,
    pub reasoning_effort: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub workspace: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub llm: LlmSettings,
}

pub fn read_preferences(path: &Path) -> Preferences {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn save_preferences(path: &Path, prefs: &Preferences) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(prefs) {
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}
struct Inner {
    ws: Option<Workstation>,
    sessions: VecDeque<(String, Arc<Session>)>,
    watcher: Option<notify::RecommendedWatcher>,
    generation: u64,
}
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<Inner>>,
    events: broadcast::Sender<Value>,
    pub token: String,
    pub origin: String,
    preferences: Option<PathBuf>,
    fonts: Vec<String>,
    llm_settings: Arc<Mutex<LlmSettings>>,
    secrets: secrets::Secrets,
    setter: setter::SetterState,
}
fn err(code: &str, message: impl ToString) -> Error {
    Error {
        code: code.into(),
        message: message.to_string(),
    }
}
fn status(code: &str) -> StatusCode {
    match code {
        "not_found" => StatusCode::NOT_FOUND,
        "invalid" | "invalid_id" => StatusCode::BAD_REQUEST,
        "invalid_file" | "compile_failed" | "too_many_problems" => StatusCode::UNPROCESSABLE_ENTITY,
        "path_escape" | "disallowed_path" | "read_only" => StatusCode::FORBIDDEN,
        "contest_frozen" | "contest_archived" => StatusCode::LOCKED,
        "upstream" => StatusCode::BAD_GATEWAY,
        "keychain" => StatusCode::INTERNAL_SERVER_ERROR,
        "no_compiler" | "not_configured" => StatusCode::PRECONDITION_FAILED,
        "conflict" | "not_compiled" | "no_workspace" | "problem_referenced" | "already_frozen"
        | "not_frozen" => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
fn error_response(e: Error) -> Response {
    (status(&e.code), axum::Json(e)).into_response()
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| err("invalid", format!("missing {key}")))
}
fn number(q: &HashMap<String, String>, key: &str, default: f64) -> Result<f64, Error> {
    let n = q
        .get(key)
        .map(|s| s.parse::<f64>())
        .transpose()
        .map_err(|_| err("invalid", format!("invalid {key}")))?
        .unwrap_or(default);
    if n.is_finite() {
        Ok(n)
    } else {
        Err(err("invalid", format!("invalid {key}")))
    }
}
fn expand_path(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(path.strip_prefix("~/").unwrap_or(""))
                .to_string_lossy()
                .into_owned();
        }
    }
    path.into()
}

fn validate_base_url(base_url: &str) -> Result<(), Error> {
    let url = url::Url::parse(base_url).map_err(|e| err("invalid", e))?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(err(
            "invalid",
            "enter an HTTP(S) API base URL without credentials",
        ));
    }
    Ok(())
}

impl AppState {
    pub fn new(origin: String, preferences: Option<PathBuf>, fonts: Vec<String>) -> Self {
        let (events, _) = broadcast::channel(128);
        let llm_settings = preferences
            .as_deref()
            .map(read_preferences)
            .map(|prefs| prefs.llm)
            .unwrap_or_default();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                ws: None,
                sessions: VecDeque::new(),
                watcher: None,
                generation: 0,
            })),
            events,
            token: uuid::Uuid::new_v4().to_string(),
            origin,
            secrets: secrets::Secrets::new(preferences.as_deref()),
            preferences,
            fonts,
            llm_settings: Arc::new(Mutex::new(llm_settings)),
            setter: setter::SetterState::default(),
        }
    }
    pub fn restore(&self, workspace: &str) -> Result<(), Error> {
        self.open(Workstation::open(expand_path(workspace), None)?)
    }
    fn open(&self, mut ws: Workstation) -> Result<(), Error> {
        ws.set_font_dirs(self.fonts.clone());
        let root = ws.root();
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        let generation = inner.generation;
        let state = self.clone();
        // The callback owns only a sender, preventing a state/watcher ownership cycle.
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let _ = tx.send(event);
            })
            .map_err(|e| err("watch", e))?;
        watcher
            .watch(Path::new(&root), RecursiveMode::Recursive)
            .map_err(|e| err("watch", e))?;
        let watch_root = PathBuf::from(&root);
        std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut batch = vec![first];
                std::thread::sleep(Duration::from_millis(150));
                batch.extend(rx.try_iter());
                let mut paths = Vec::new();
                for event in batch.into_iter().flatten() {
                    if matches!(event.kind, notify::EventKind::Access(_)) {
                        continue;
                    }
                    for path in event.paths {
                        let Ok(rel) = path.strip_prefix(&watch_root) else {
                            continue;
                        };
                        let path = rel.to_string_lossy().replace('\\', "/");
                        if rel.components().any(|p| {
                            matches!(
                                p.as_os_str().to_str(),
                                Some(".git" | ".jj" | "build" | "node_modules" | ".xcpc")
                            )
                        }) || path.ends_with('~')
                            || path.ends_with(".swp")
                            || path.contains(".tmp")
                        {
                            continue;
                        }
                        if !paths.contains(&path) {
                            paths.push(path);
                        }
                    }
                }
                if paths.is_empty() {
                    continue;
                }
                let mut guard = state.inner.lock().unwrap();
                if guard.generation != generation {
                    break;
                }
                if let Some(ws) = &mut guard.ws {
                    let _ = ws.refresh(None);
                }
                drop(guard);
                let _ = state.events.send(json!({"type":"changed", "paths":paths}));
            }
        });
        inner.ws = Some(ws);
        inner.sessions.clear();
        inner.watcher = Some(watcher);
        drop(inner);
        if let Some(path) = &self.preferences {
            let mut prefs = read_preferences(path);
            prefs.workspace = Some(root);
            save_preferences(path, &prefs);
        }
        let _ = self.events.send(json!({"type":"workspace"}));
        Ok(())
    }
    fn with_ws<T>(&self, f: impl FnOnce(&Workstation) -> Result<T, Error>) -> Result<T, Error> {
        let inner = self.inner.lock().unwrap();
        f(inner
            .ws
            .as_ref()
            .ok_or_else(|| err("no_workspace", "no workspace open"))?)
    }
    fn session(&self, kind: &str, id: &str, editorial: bool) -> Result<Arc<Session>, Error> {
        let key = format!("{kind}:{id}:{editorial}");
        let mut inner = self.inner.lock().unwrap();
        let entry = if let Some(i) = inner.sessions.iter().position(|(k, _)| k == &key) {
            inner.sessions.remove(i).unwrap().1
        } else {
            let ws = inner
                .ws
                .as_ref()
                .ok_or_else(|| err("no_workspace", "no workspace open"))?;
            Arc::new(ws.session(
                kind.into(),
                id.into(),
                Some(SessionOptions {
                    editorial: Some(editorial),
                    language: None,
                }),
            )?)
        };
        inner.sessions.push_back((key, entry.clone()));
        while inner.sessions.len() > 4 {
            inner.sessions.pop_front();
        }
        Ok(entry)
    }

    fn current_llm_settings(&self) -> LlmSettings {
        self.llm_settings.lock().unwrap().clone()
    }

    fn save_llm_settings(&self, value: &Value) -> Result<Value, Error> {
        let settings = LlmSettings {
            api_format: value
                .get("apiFormat")
                .and_then(Value::as_str)
                .unwrap_or("chat")
                .to_owned(),
            base_url: value
                .get("baseUrl")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .trim_end_matches('/')
                .to_owned(),
            model: value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned(),
            reasoning_effort: value
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .unwrap_or("auto")
                .to_owned(),
        };
        validate_base_url(&settings.base_url)?;
        if xcpc_setter::llm::ApiFormat::parse(&settings.api_format).is_none() {
            return Err(err("invalid", "unsupported API format"));
        }
        if !matches!(
            settings.reasoning_effort.as_str(),
            "auto" | "minimal" | "low" | "medium" | "high"
        ) {
            return Err(err("invalid", "unsupported reasoning effort"));
        }

        if let Some(api_key) = value.get("apiKey").and_then(Value::as_str) {
            if !api_key.trim().is_empty() {
                self.secrets.set(api_key.trim())?;
            }
        }
        if value.get("clearApiKey").and_then(Value::as_bool) == Some(true) {
            self.secrets.clear()?;
        }

        let mut guard = self.llm_settings.lock().unwrap();
        *guard = settings.clone();
        if let Some(path) = &self.preferences {
            let mut prefs = read_preferences(path);
            prefs.llm = settings;
            save_preferences(path, &prefs);
        }
        Ok(self.llm_settings_response(&guard))
    }

    fn llm_settings_response(&self, settings: &LlmSettings) -> Value {
        json!({
            "apiFormat": if settings.api_format.is_empty() { "chat" } else { settings.api_format.as_str() },
            "baseUrl": settings.base_url,
            "model": settings.model,
            "reasoningEffort": settings.reasoning_effort,
            "apiKeyConfigured": self.secrets.get().ok().flatten().is_some(),
        })
    }

    /// The configured model client.
    fn llm(&self) -> Result<xcpc_setter::Llm, Error> {
        let s = self.current_llm_settings();
        if s.base_url.is_empty() {
            return Err(err("not_configured", "请先在设置里填写模型服务"));
        }
        validate_base_url(&s.base_url)?;
        Ok(xcpc_setter::Llm {
            format: xcpc_setter::llm::ApiFormat::parse(&s.api_format).unwrap_or_default(),
            base_url: s.base_url,
            model: s.model,
            api_key: self.secrets.get()?,
            reasoning_effort: s.reasoning_effort,
        })
    }

    fn llm_models(&self) -> Result<Value, Error> {
        let models = self.llm()?.models().map_err(|e| err(&e.code, e.message))?;
        Ok(json!({ "models": models }))
    }

    pub fn native_pdf(&self, url: &str) -> Result<(Vec<u8>, String), Error> {
        let url = url::Url::parse(url).map_err(|e| err("invalid", e))?;
        if url.origin().ascii_serialization() != self.origin {
            return Err(err("invalid", "invalid export URL"));
        }
        let parts: Vec<_> = url
            .path_segments()
            .ok_or_else(|| err("invalid", "invalid PDF URL"))?
            .collect();
        if parts.len() != 4 || parts[0] != "api" || parts[1] != "pdf" {
            return Err(err("invalid", "invalid PDF URL"));
        }
        let decode = |s: &str| {
            url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
                .next()
                .unwrap()
                .1
                .into_owned()
        };
        let editorial = url.query_pairs().any(|(k, v)| k == "editorial" && v == "1");
        let session = self.session(&decode(parts[2]), &decode(parts[3]), editorial)?;
        let name = session
            .default_output()
            .split('/')
            .next_back()
            .unwrap_or("document.pdf")
            .to_string();
        Ok((session.export_pdf()?, name))
    }
    fn dispatch(&self, method: Method, uri: Uri, body: Bytes) -> Result<Response, Error> {
        let path = uri.path();
        let q: HashMap<String, String> =
            url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        let value: Value = if body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&body).map_err(|e| err("invalid", e))?
        };
        let parts: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                // Decode one URL path segment without treating '+' as a space.
                url::form_urlencoded::parse(format!("x={}", s.replace('+', "%2B")).as_bytes())
                    .next()
                    .map(|(_, s)| s.into_owned())
                    .unwrap_or_default()
            })
            .collect();
        let p: Vec<&str> = parts.iter().map(String::as_str).collect();
        let param = |key: &str| {
            q.get(key)
                .map(String::as_str)
                .ok_or_else(|| err("invalid", format!("missing {key}")))
        };
        let result = match (method.as_str(), p.as_slice()) {
            ("GET", ["api", "settings", "llm"]) => {
                let settings = self.current_llm_settings();
                self.llm_settings_response(&settings)
            }
            ("PUT", ["api", "settings", "llm"]) => self.save_llm_settings(&value)?,
            ("GET", ["api", "llm", "models"]) => self.llm_models()?,
            (_, ["api", "setter", ..]) | ("GET", ["api", "jobs", _]) => return self.setter_route(method.as_str(), &p, &q, &value),
            ("GET", ["api", "workspace"]) => {
                let inner = self.inner.lock().unwrap();
                match &inner.ws { Some(ws) => json!({"root":ws.root(),"readOnly":ws.read_only(),"version":env!("CARGO_PKG_VERSION")}), None => json!({"root":null,"version":env!("CARGO_PKG_VERSION")}) }
            }
            ("POST", ["api", "workspace", action @ ("open" | "init" | "example")]) => {
                let root = expand_path(text(&value, "path")?);
                let ws = match *action {
                    "open" => Workstation::open(root, Some(OpenOptions { read_only: value["readOnly"].as_bool(), font_dirs: None }))?,
                    "init" => Workstation::init(root, value["template"].as_str().map(str::to_string))?,
                    _ => Workstation::init_example(root, value["language"].as_str().map(str::to_string))?,
                };
                let root = ws.root(); self.open(ws)?; json!({"root":root})
            }
            ("POST", ["api", "contests"]) => {
                let name = text(&value, "name")?.trim();
                if name.is_empty() {
                    return Err(err("invalid", "contest name cannot be empty"));
                }
                let id = format!("round-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
                self.with_ws(|ws| ws.create_contest(id, name.to_owned()))?
            }
            ("GET", ["api", "problems"]) => self.with_ws(|ws| ws.problems(Some(json!({
                "text":q.get("q"), "tags":q.get("tags").map(|s| s.split(',').collect::<Vec<_>>()).unwrap_or_default(),
                "ratingMin":q.get("min").map(|s| s.parse::<u32>()).transpose().map_err(|_| err("invalid", "invalid min rating"))?,
                "ratingMax":q.get("max").map(|s| s.parse::<u32>()).transpose().map_err(|_| err("invalid", "invalid max rating"))?
            }))))?,
            ("GET", ["api", "problems", id]) => self.with_ws(|ws| ws.problem((*id).into()))?,
            ("PUT", ["api", "problems", id, "samples"]) => self.with_ws(|ws| { ws.save_samples((*id).into(), value)?; Ok(ws.problem((*id).into())?["samples"].clone()) })?,
            ("GET", ["api", "contests"]) => self.with_ws(|ws| ws.contests())?,
            ("GET", ["api", "contests", id]) => self.with_ws(|ws| ws.contest((*id).into()))?,
            ("GET", ["api", "search"]) => self.with_ws(|ws| ws.fuzzy(q.get("q").cloned().unwrap_or_default(), Some(12)))?,
            ("GET", ["api", "file"]) => self.with_ws(|ws| ws.read(param("path")?.into()))?,
            ("PUT", ["api", "file"]) => self.with_ws(|ws| ws.write(text(&value,"path")?.into(), text(&value,"text")?.into(), value["hash"].as_str().map(str::to_string)).map(|hash| json!({"hash":hash})))?,
            ("GET", ["api", "asset"]) => {
                let path = param("path")?;
                let bytes = self.with_ws(|ws| ws.read_bytes(path.into()))?;
                let mime = mime_guess::from_path(path).first_or_octet_stream().to_string();
                return Ok(([("content-type", mime), ("x-content-type-options", "nosniff".into())], bytes).into_response());
            }
            ("POST", ["api", "compile", kind, id]) => {
                let session = self.session(kind,id,q.get("editorial").is_some_and(|s| s == "1"))?;
                let unsaved: Option<Unsaved> = serde_json::from_value(value.get("unsaved").cloned().unwrap_or(Value::Null)).map_err(|e| err("invalid", e))?;
                let seq = session.next_sequence(); session.compile(unsaved,seq)?
            }
            ("GET", ["api", "page", kind, id, page]) => {
                let session = self.session(kind,id,q.get("editorial").is_some_and(|s| s == "1"))?;
                let page = page.parse::<u32>().map_err(|_| err("invalid", "invalid page"))?;
                let revision = q.get("rev").map(|s| s.parse::<u64>()).transpose().map_err(|_| err("invalid", "invalid revision"))?;
                let png = session.render(page,number(&q,"scale",2.0)?,revision)?;
                // Do not use immutable browser caches across workspace switches.
                return Ok(([("content-type","image/png"),("cache-control","no-store")],png).into_response());
            }
            ("GET", ["api", "pdf", kind, id]) => {
                let session = self.session(kind,id,q.get("editorial").is_some_and(|s| s == "1"))?;
                let bytes = session.export_pdf()?;
                let name = session.default_output().split('/').next_back().unwrap_or("document.pdf").to_string();
                let mode = if q.get("download").is_some_and(|v| v == "1") { "attachment" } else { "inline" };
                let encoded: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect();
                return Ok(([("content-type","application/pdf".to_string()),("content-disposition",format!("{mode}; filename*=UTF-8''{}",encoded.replace('+',"%20")))],bytes).into_response());
            }
            ("GET", ["api", "locate", kind, id, page]) => {
                let session = self.session(kind,id,q.get("editorial").is_some_and(|s| s == "1"))?;
                session.locate(page.parse().map_err(|_| err("invalid","invalid page"))?,number(&q,"x",0.0)?,number(&q,"y",0.0)?)?
            }
            (_, ["api", ..]) => return Err(err("not_found", "unknown API route")),
            ("GET", _) => {
                let name = path.trim_start_matches('/');
                let asset = Web::get(name).or_else(|| if !name.contains('.') { Web::get("index.html") } else { None }).ok_or_else(|| err("not_found","file not found"))?;
                let mime = if Web::get(name).is_none() { "text/html".into() } else { mime_guess::from_path(name).first_or_octet_stream().to_string() };
                return Ok(([("content-type",mime),("cache-control","no-cache".into())],asset.data.into_owned()).into_response());
            }
            _ => return Err(err("not_found","unknown route")),
        };
        Ok(axum::Json(result).into_response())
    }
}
async fn auth(State(s): State<AppState>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    if headers.get("host").and_then(|v| v.to_str().ok()) != s.origin.strip_prefix("http://") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if origin != s.origin {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let token_param = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned());
    if request.method() == Method::GET
        && request.uri().path() == "/"
        && token_param.as_deref() == Some(s.token.as_str())
    {
        let mut response = axum::response::Redirect::to("/").into_response();
        response.headers_mut().insert(
            "set-cookie",
            HeaderValue::from_str(&format!(
                "xcpc_token={}; HttpOnly; SameSite=Strict; Path=/",
                s.token
            ))
            .unwrap(),
        );
        return response;
    }
    let authorized = headers.get("x-xcpc-token").and_then(|v| v.to_str().ok())
        == Some(s.token.as_str())
        || headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|c| {
                c.split(';')
                    .any(|part| part.trim() == format!("xcpc_token={}", s.token))
            });
    if !authorized {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("x-frame-options", HeaderValue::from_static("DENY"));
    response
        .headers_mut()
        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    response.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'self'; frame-ancestors 'none'"));
    response
}
async fn handle(State(state): State<AppState>, method: Method, uri: Uri, body: Bytes) -> Response {
    match tokio::task::spawn_blocking(move || state.dispatch(method, uri, body)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => error_response(e),
        Err(e) => error_response(err("internal", e)),
    }
}
async fn events(State(state): State<AppState>) -> impl IntoResponse {
    let stream = tokio_stream::wrappers::BroadcastStream::new(state.events.subscribe()).filter_map(
        |v| async move {
            v.ok().map(|v| {
                Ok::<_, Infallible>(
                    Event::default()
                        .event(v["type"].as_str().unwrap_or("changed"))
                        .data(v.to_string()),
                )
            })
        },
    );
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(25))
            .event(Event::default().event("ping")),
    )
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/events", get(events))
        .fallback(any(handle))
        .layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state)
}

/// Serves `state` on `listener` until the process ends.
pub async fn serve(listener: tokio::net::TcpListener, state: AppState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}
