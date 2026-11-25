import { Suspense, lazy, useEffect, useState } from "react";
import { api, type WorkspaceInfo } from "./api";
import { Brand, Icon } from "./components/Icon";
import { CommandPalette } from "./components/CommandPalette";
import { href, navigate, useRoute } from "./lib/router";
import { useWorkspaceEvents } from "./lib/hooks";
import { Library } from "./views/Library";
import { Contests } from "./views/Contests";
import { Settings } from "./views/Settings";
import { Welcome } from "./views/Welcome";

// The editor (CodeMirror, Vim, KaTeX) is most of the bundle: load it on first use.
const Studio = lazy(() => import("./views/Studio").then((m) => ({ default: m.Studio })));
const Setter = lazy(() => import("./views/Setter").then((m) => ({ default: m.Setter })));

export function App() {
  const route = useRoute();
  const [ws, setWs] = useState<WorkspaceInfo>();
  const [palette, setPalette] = useState(false);

  const loadWs = () => api.workspace().then(setWs, () => setWs({ root: null, version: "" }));
  useEffect(() => {
    loadWs();
  }, []);
  useWorkspaceEvents((paths) => paths[0] === "*" && loadWs());

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!ws) return null;
  if (!ws.root || route.name === "welcome") return <Welcome onOpened={() => loadWs().then(() => navigate("/", true))} />;

  if (route.name === "setter") {
    return (
      <>
        <Suspense fallback={null}>
          <Setter contest={route.contest} />
        </Suspense>
        {palette && <CommandPalette onClose={() => setPalette(false)} />}
      </>
    );
  }

  if (route.name === "studio") {
    return (
      <>
        <Suspense fallback={null}>
          <Studio key={`${route.kind}:${route.id}`} kind={route.kind} id={route.id} editorial={route.editorial} onPalette={() => setPalette(true)} />
        </Suspense>
        {palette && <CommandPalette onClose={() => setPalette(false)} />}
      </>
    );
  }

  const name = ws.root.split(/[\\/]/).filter(Boolean).pop();
  const link = (to: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    navigate(to);
  };
  return (
    <div className="shell">
      <header className="topbar">
        <div className="brand">
          <Brand />
          <span className="hide-mobile">XCPC</span>
          <span className="workspace-name" title={ws.root}>
            {name}
          </span>
        </div>
        <nav className="nav">
          <a href="/" onClick={link("/")} aria-current={route.name === "library" ? "page" : undefined}>
            题库
          </a>
          <a href={href({ name: "contests" })} onClick={link("/contests")} aria-current={route.name === "contests" ? "page" : undefined}>
            比赛
          </a>
        </nav>
        <div className="spacer" />
        <button className="search-trigger" onClick={() => setPalette(true)} aria-label="搜索">
          <Icon name="search" />
          <span>跳转到题目或比赛</span>
          <kbd>{navigator.platform.includes("Mac") ? "⌘K" : "Ctrl K"}</kbd>
        </button>
        <button className="btn icon" title="切换工作区" onClick={() => navigate("/welcome")}>
          <Icon name="folder" />
        </button>
        <button className="btn icon" title="设置" aria-pressed={route.name === "settings"} onClick={() => navigate("/settings")}>
          <Icon name="settings" />
        </button>
        {!ws.readOnly && (
          <button className="btn primary" onClick={() => navigate({ name: "setter", contest: null })}>
            <Icon name="sparkle" />
            <span className="hide-mobile">出题</span>
          </button>
        )}
      </header>
      <main style={{ minHeight: 0 }}>
        {route.name === "contests" ? <Contests /> : route.name === "settings" ? <Settings /> : <Library />}
      </main>
      {palette && <CommandPalette onClose={() => setPalette(false)} />}
    </div>
  );
}
