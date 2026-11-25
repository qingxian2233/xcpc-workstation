// Watches the workspace for changes made outside the app (git pull, an AI agent over MCP,
// another editor) and reports them in batches.
import { watch } from "node:fs";

const IGNORED = /(^|\/)(\.git|\.jj|build|node_modules|\.xcpc)(\/|$)|~$|\.tmp[^/]*$|\.swp$/;

export function watchWorkspace(root, onChange, delay = 150) {
  let pending = new Set();
  let timer = null;
  let watcher;
  try {
    watcher = watch(root, { recursive: true }, (_event, file) => {
      if (!file) return;
      const rel = file.split("\\").join("/");
      if (IGNORED.test(rel)) return;
      pending.add(rel);
      clearTimeout(timer);
      timer = setTimeout(() => {
        const paths = [...pending];
        pending = new Set();
        onChange(paths);
      }, delay);
    });
  } catch (err) {
    console.warn(`file watching unavailable: ${err.message}`);
    return () => {};
  }
  return () => {
    clearTimeout(timer);
    watcher.close();
  };
}
