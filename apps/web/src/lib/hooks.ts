import { useCallback, useEffect, useRef, useState } from "react";
import { subscribe } from "../api";

/** Loads `fn()` and reloads it when `deps` change or the workspace changes on disk. */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[]): { data: T | undefined; error: Error | undefined; reload: () => void } {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<Error>();
  const [tick, setTick] = useState(0);
  const reload = useCallback(() => setTick((t) => t + 1), []);
  useEffect(() => {
    let live = true;
    fn().then(
      (d) => live && (setData(d), setError(undefined)),
      (e) => live && setError(e),
    );
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  useWorkspaceEvents(reload);
  return { data, error, reload };
}

export function useWorkspaceEvents(fn: (paths: string[]) => void) {
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => subscribeShared((paths) => ref.current(paths)), []);
}

// One EventSource for the whole page.
const handlers = new Set<(paths: string[]) => void>();
let close: (() => void) | null = null;
function subscribeShared(h: (paths: string[]) => void) {
  handlers.add(h);
  if (!close) close = subscribe((type, data) => handlers.forEach((fn) => fn(type === "workspace" ? ["*"] : data.paths ?? [])));
  return () => {
    handlers.delete(h);
    if (handlers.size === 0) {
      close?.();
      close = null;
    }
  };
}

export function useMedia(query: string): boolean {
  const [m, setM] = useState(() => matchMedia(query).matches);
  useEffect(() => {
    const mq = matchMedia(query);
    const on = () => setM(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, [query]);
  return m;
}

export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/** Persisted UI preference. */
export function usePref<T>(key: string, initial: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const s = localStorage.getItem(`xcpc:${key}`);
      return s === null ? initial : JSON.parse(s);
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (n: T) => {
      setV(n);
      localStorage.setItem(`xcpc:${key}`, JSON.stringify(n));
    },
    [key],
  );
  return [v, set];
}
