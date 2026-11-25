// The only native capabilities the web UI gets.
const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("xcpcDesktop", {
  platform: process.platform,
  pickFolder: () => ipcRenderer.invoke("pick-folder"),
});
