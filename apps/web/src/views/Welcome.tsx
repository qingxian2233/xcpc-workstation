import { useState } from "react";
import { api } from "../api";
import { Brand, Icon } from "../components/Icon";

export function Welcome({ onOpened }: { onOpened(): void }) {
  const [path, setPath] = useState(localStorage.getItem("xcpc:last-path") ?? "");
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const run = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    setError(undefined);
    try {
      await fn();
      localStorage.setItem("xcpc:last-path", path);
      onOpened();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="welcome">
      <div className="box">
        <div className="brand" style={{ gap: 12 }}>
          <span style={{ width: 40, height: 40, display: "block" }}>
            <Brand />
          </span>
        </div>
        <h1>XCPC Workstation</h1>
        <p>工作区就是一个普通文件夹：题目、样例和比赛都是可读的文本，适合用 Git 管理。</p>
        <form
          className="row2"
          onSubmit={(e) => {
            e.preventDefault();
            run(() => api.openWorkspace(path.trim()));
          }}
        >
          <label className="field">
            <Icon name="folder" />
            <input value={path} onChange={(e) => setPath(e.target.value)} placeholder="工作区路径，例如 ~/xcpc" autoFocus spellCheck={false} />
          </label>
          {window.xcpcDesktop && (
            <button
              type="button"
              className="btn outline"
              onClick={async () => {
                const p = await window.xcpcDesktop!.pickFolder();
                if (p) setPath(p);
              }}
            >
              浏览…
            </button>
          )}
          <button className="btn primary" disabled={!path.trim() || busy}>
            打开
          </button>
        </form>
        <div className="row2">
          <button className="btn outline" disabled={!path.trim() || busy} onClick={() => run(() => api.initWorkspace(path.trim()))}>
            在此新建空工作区
          </button>
          <button className="btn outline" disabled={!path.trim() || busy} onClick={() => run(() => api.exampleWorkspace(path.trim(), "zh"))}>
            在此释放示例
          </button>
        </div>
        {error && <div className="error">{error}</div>}
      </div>
    </div>
  );
}
