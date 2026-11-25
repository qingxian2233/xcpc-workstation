import { createRoot } from "react-dom/client";
import "katex/dist/katex.min.css";
import "./styles.css";
import { App } from "./App";

declare global {
  interface Window {
    xcpcDesktop?: { platform: string; pickFolder(): Promise<string | null> };
  }
}
// The desktop shell draws no title bar: leave room for the window controls.
if (window.xcpcDesktop) document.documentElement.dataset.desktop = window.xcpcDesktop.platform;

createRoot(document.getElementById("root")!).render(<App />);
