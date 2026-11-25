//! Runs the app's HTTP API without a window, for tests and browser-based development:
//! `xcpc-serve [workspace] [--port N]` prints `{"origin": …, "token": …}` and serves.

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mut workspace = std::env::var("XCPC_WORKSPACE").ok();
    let mut port = 0u16;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => {
                port = args
                    .next()
                    .and_then(|p| p.parse().ok())
                    .expect("--port <number>")
            }
            // What scripts/test-tauri.py passes to the app binary; headless already.
            "--serve-test" => {}
            _ => workspace = Some(a),
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
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
    let prefs = std::env::var_os("XCPC_PREFERENCES").map(std::path::PathBuf::from);
    let state = xcpc_server::AppState::new(origin.clone(), prefs, fonts);
    if let Some(w) = workspace {
        if let Err(e) = state.restore(&w) {
            eprintln!("cannot open {w}: {e}");
        }
    }
    println!(
        "{}",
        serde_json::json!({ "origin": origin, "token": state.token, "url": format!("{origin}/?token={}", state.token) })
    );
    xcpc_server::serve(listener, state)
        .await
        .expect("HTTP service");
}
