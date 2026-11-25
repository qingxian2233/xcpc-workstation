// Desktop-only compatibility: keep apps/web byte-for-byte identical to the source UI.
if (window.top === window.self) {
  Object.defineProperty(window, "xcpcDesktop", {
    value: Object.freeze({
      platform: "darwin",
      pickFolder: () => window.__TAURI_INTERNALS__.invoke("pick_folder"),
    }),
    writable: false,
  });
  // Electron's CSS drag regions are implemented by the Tauri native window.
  document.addEventListener("mousedown", (event) => {
    if (event.button === 0 && event.target instanceof Element &&
        (event.target.matches(".topbar, .topbar .spacer"))) {
      window.__TAURI_INTERNALS__.invoke("plugin:window|start_dragging", { label: "main" });
    }
  });
}

// Native PDF preview and save dialogs preserve the existing export buttons.
document.addEventListener("click", async (event) => {
  const link = event.target instanceof Element ? event.target.closest("a[href]") : null;
  if (!link) return;
  const url = new URL(link.href, location.href);
  if (url.origin !== location.origin || !url.pathname.startsWith("/api/pdf/")) return;
  event.preventDefault();
  try {
    await window.__TAURI_INTERNALS__.invoke("export_pdf", { url: url.href, download: url.searchParams.get("download") === "1" });
  } catch (error) {
    window.alert(String(error));
  }
}, true);
