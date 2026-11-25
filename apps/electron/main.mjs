// Desktop shell. The window is just a browser on the same server phones and browsers use;
// the server runs inside this process, bound to loopback with a per-launch token.
import { app, BrowserWindow, dialog, ipcMain, shell } from "electron";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { start } from "@xcpc/server/src/serve.js";

const here = dirname(fileURLToPath(import.meta.url));
const prefsFile = () => join(app.getPath("userData"), "prefs.json");
const readPrefs = () => {
  try {
    return JSON.parse(readFileSync(prefsFile(), "utf8"));
  } catch {
    return {};
  }
};
const writePrefs = (patch) => writeFileSync(prefsFile(), JSON.stringify({ ...readPrefs(), ...patch }, null, 2));

let server;

async function createWindow() {
  const prefs = readPrefs();
  server ??= await start({
    workspace: process.env.XCPC_WORKSPACE ?? prefs.workspace,
    token: true,
    onWorkspace: (root) => writePrefs({ workspace: root }),
  });

  const bounds = prefs.bounds ?? { width: 1320, height: 840 };
  const win = new BrowserWindow({
    ...bounds,
    minWidth: 760,
    minHeight: 520,
    show: false,
    backgroundColor: "#f6f5f2",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "hidden",
    titleBarOverlay: process.platform === "darwin" ? undefined : { color: "#00000000", symbolColor: "#57544d", height: 52 },
    webPreferences: { preload: join(here, "preload.cjs"), contextIsolation: true, sandbox: true },
  });
  win.once("ready-to-show", () => win.show());
  win.on("close", () => writePrefs({ bounds: win.getBounds() }));

  // PDFs open in their own window (Chromium's viewer); other links go to the browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith(new URL(server.url).origin)) return { action: "allow", overrideBrowserWindowOptions: { width: 900, height: 1000, titleBarStyle: "default" } };
    shell.openExternal(url);
    return { action: "deny" };
  });

  await win.loadURL(server.url);
}

ipcMain.handle("pick-folder", async (e) => {
  const win = BrowserWindow.fromWebContents(e.sender);
  const r = await dialog.showOpenDialog(win, { properties: ["openDirectory", "createDirectory"] });
  return r.canceled ? null : r.filePaths[0];
});

app.whenReady().then(createWindow);
app.on("activate", () => BrowserWindow.getAllWindows().length === 0 && createWindow());
app.on("window-all-closed", () => {
  if (process.platform !== "darwin") app.quit();
});
app.on("quit", () => server?.close());
