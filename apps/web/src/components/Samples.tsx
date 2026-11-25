import { useEffect, useState } from "react";
import { api, type ProblemDetail } from "../api";
import { Icon } from "./Icon";

export function Samples({ problem, readOnly, onSaved }: { problem: ProblemDetail; readOnly: boolean; onSaved(s: ProblemDetail["samples"]): void }) {
  const [pairs, setPairs] = useState(problem.samples.map((s) => ({ input: s.input, output: s.output })));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  useEffect(() => setPairs(problem.samples.map((s) => ({ input: s.input, output: s.output }))), [problem]);

  const dirty = JSON.stringify(pairs) !== JSON.stringify(problem.samples.map((s) => ({ input: s.input, output: s.output })));
  const set = (i: number, k: "input" | "output", v: string) => setPairs((p) => p.map((x, j) => (j === i ? { ...x, [k]: v } : x)));

  const save = async () => {
    setBusy(true);
    setError(undefined);
    try {
      onSaved(await api.saveSamples(problem.id, pairs));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="samples">
      {pairs.map((p, i) => (
        <div className="sample" key={i}>
          <header>
            <span className="num">样例 {i + 1}</span>
            {!readOnly && (
              <button className="btn icon" title="删除" onClick={() => setPairs(pairs.filter((_, j) => j !== i))}>
                <Icon name="trash" />
              </button>
            )}
          </header>
          <textarea value={p.input} readOnly={readOnly} spellCheck={false} placeholder="输入" onChange={(e) => set(i, "input", e.target.value)} />
          <textarea value={p.output} readOnly={readOnly} spellCheck={false} placeholder="输出" onChange={(e) => set(i, "output", e.target.value)} />
        </div>
      ))}
      {!readOnly && (
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn outline" onClick={() => setPairs([...pairs, { input: "", output: "" }])}>
            <Icon name="plus" /> 添加样例
          </button>
          <span style={{ flex: 1 }} />
          {error && <span style={{ color: "var(--danger)", alignSelf: "center", fontSize: 13 }}>{error}</span>}
          <button className="btn primary" disabled={!dirty || busy} onClick={save}>
            保存样例
          </button>
        </div>
      )}
    </div>
  );
}
