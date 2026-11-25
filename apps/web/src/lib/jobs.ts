import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Job } from "../api";

/**
 * Runs a server job and follows it: `run(start)` resolves with the result (or throws),
 * while `job` mirrors its progress for the UI. Starting another job abandons the last.
 */
export function useJob<T>() {
  const [job, setJob] = useState<Job<T> | null>(null);
  const [running, setRunning] = useState(false);
  const token = useRef(0);
  useEffect(() => () => void (token.current = -1), []);

  const run = useCallback(async (start: () => Promise<{ job: string }>, onStarted?: (id: string) => void): Promise<T> => {
    const mine = ++token.current;
    setRunning(true);
    setJob({ done: false, stage: "", round: 0, report: null, result: null, error: null });
    try {
      const { job: id } = await start();
      onStarted?.(id);
      for (;;) {
        await new Promise((r) => setTimeout(r, 600));
        if (token.current !== mine) throw new DOMException("abandoned", "AbortError");
        const j = await api.job<T>(id);
        if (token.current !== mine) throw new DOMException("abandoned", "AbortError");
        setJob(j);
        if (j.done) {
          if (j.error) throw Object.assign(new Error(j.error.message), { code: j.error.code });
          return j.result as T;
        }
      }
    } finally {
      if (token.current === mine) setRunning(false);
    }
  }, []);

  const cancel = useCallback(() => {
    token.current++;
    setRunning(false);
    setJob(null);
  }, []);

  return { job, running, run, cancel };
}
