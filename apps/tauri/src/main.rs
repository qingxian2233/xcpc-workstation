#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use xcpc_server as server;
use std::sync::Arc;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
async fn pick_folder() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .set_title("选择工作区")
        .pick_folder()
        .await
        .map(|p| p.path().to_string_lossy().into_owned())
}

#[tauri::command]
async fn export_pdf(
    app: tauri::AppHandle,
    state: tauri::State<'_, server::AppState>,
    url: String,
    download: bool,
) -> Result<(), String> {
    let backend = state.inner().clone();
    let (bytes, name) = tauri::async_runtime::spawn_blocking(move || backend.native_pdf(&url))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if download {
        if let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("导出 PDF")
            .set_file_name(&name)
            .add_filter("PDF", &["pdf"])
            .save_file()
            .await
        {
            file.write(&bytes).await.map_err(|e| e.to_string())?;
        }
    } else {
        let dir = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("exports")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(name);
        std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
        open::that(file).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn main() {
    let runtime = Arc::new(tokio::runtime::Runtime::new().expect("native runtime"));
    // Headless mode exercises exactly the same native backend as the packaged app.
    if std::env::args().any(|s| s == "--serve-test") {
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("loopback listener");
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let fonts = std::env::var("XCPC_FONT_DIRS")
                .ok()
                .map(|s| {
                    std::env::split_paths(&s)
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            let state = server::AppState::new(origin.clone(), None, fonts);
            if let Ok(path) = std::env::var("XCPC_WORKSPACE") {
                state.restore(&path).expect("test workspace");
            }
            println!(
                "{}",
                serde_json::json!({"origin":origin,"token":state.token})
            );
            server::serve(listener, state)
                .await
                .expect("native HTTP service");
        });
        return;
    }
    let runtime_for_setup = runtime.clone();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![pick_folder, export_pdf])
        .setup(move |app| {
            let preferences = app.path().app_config_dir()?.join("preferences.json");
            let prefs = server::read_preferences(&preferences);
            let resource = app.path().resource_dir()?;
            let fonts = vec![resource.join("fonts/cjk").to_string_lossy().into_owned()];
            let listener =
                runtime_for_setup.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
            let origin = format!("http://{}", listener.local_addr()?);
            let state = server::AppState::new(origin.clone(), Some(preferences.clone()), fonts);
            if let Some(path) = std::env::var("XCPC_WORKSPACE").ok().or(prefs.workspace) {
                if let Err(e) = state.restore(&path) {
                    eprintln!("cannot reopen workspace: {e}");
                }
            }
            app.manage(state.clone());
            let initial_url = format!("{origin}/?token={}", state.token);
            runtime_for_setup.spawn(async move {
                if let Err(e) = server::serve(listener, state).await {
                    eprintln!("native service: {e}");
                }
            });
            let nav_origin = origin.clone();
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::External(initial_url.parse()?))
                    .title("XCPC Workstation")
                    .inner_size(prefs.width.unwrap_or(1320.0), prefs.height.unwrap_or(840.0))
                    .min_inner_size(760.0, 520.0)
                    .title_bar_style(tauri::TitleBarStyle::Overlay)
                    .hidden_title(true)
                    .background_color(tauri::window::Color(246, 245, 242, 255))
                    .initialization_script(include_str!("bootstrap.js"))
                    .on_navigation(move |url| {
                        if url.origin().ascii_serialization() == nav_origin {
                            true
                        } else {
                            if matches!(url.scheme(), "http" | "https" | "mailto") {
                                let _ = open::that(url.as_str());
                            }
                            false
                        }
                    })
                    .build()?;
            let win = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Resized(_) = event {
                    if let Ok(size) = win.inner_size() {
                        if let Ok(scale) = win.scale_factor() {
                            let mut prefs = server::read_preferences(&preferences);
                            prefs.width = Some(size.width as f64 / scale);
                            prefs.height = Some(size.height as f64 / scale);
                            server::save_preferences(&preferences, &prefs);
                        }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("XCPC Workstation could not start");
    drop(runtime);
}
