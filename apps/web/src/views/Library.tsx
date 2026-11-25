import { useEffect, useMemo, useState } from "react";
import { api, type CompileReport, type ProblemSummary } from "../api";
import { Icon, Rating } from "../components/Icon";
import { navigate } from "../lib/router";
import { useDebounced, useLoad } from "../lib/hooks";

const TIERS: { label: string; min?: number; max?: number }[] = [
  { label: "< 1400", max: 1399 },
  { label: "1400–1899", min: 1400, max: 1899 },
  { label: "1900–2399", min: 1900, max: 2399 },
  { label: "2400+", min: 2400 },
];

export function Library() {
  const [text, setText] = useState("");
  const [tag, setTag] = useState<string>();
  const [tier, setTier] = useState<number>();
  const q = useDebounced(text, 120);
  const range: { min?: number; max?: number } = tier == null ? {} : TIERS[tier];
  const { data: problems } = useLoad(() => api.problems({ q, tags: tag ? [tag] : undefined, min: range.min, max: range.max }), [q, tag, tier]);
  const [selected, setSelected] = useState<string>();

  const current = problems?.find((p) => p.id === selected) ?? problems?.[0];
  const open = (p: ProblemSummary) => navigate({ name: "studio", kind: "problem", id: p.id, editorial: false });

  return (
    <div className="library">
      <section className="library-main">
        <div className="toolbar">
          <label className="field">
            <Icon name="search" />
            <input value={text} onChange={(e) => setText(e.target.value)} placeholder="标题、ID 或标签" />
          </label>
          {TIERS.map((t, i) => (
            <button key={t.label} className="chip num" aria-pressed={tier === i} onClick={() => setTier(tier === i ? undefined : i)}>
              {t.label}
            </button>
          ))}
          {tag && (
            <button className="chip" aria-pressed onClick={() => setTag(undefined)}>
              {tag} ×
            </button>
          )}
          <span className="count num">{problems ? `${problems.length} 题` : ""}</span>
        </div>
        <div className="table" role="grid">
          <div className="row thead" role="row">
            <span>ID</span>
            <span>标题</span>
            <span>难度</span>
            <span>标签</span>
            <span title="有题解">解</span>
          </div>
          {problems?.map((p) => (
            <div
              key={p.id}
              className="row"
              role="row"
              tabIndex={0}
              aria-selected={current?.id === p.id}
              onClick={() => setSelected(p.id)}
              onDoubleClick={() => open(p)}
              onKeyDown={(e) => e.key === "Enter" && open(p)}
            >
              <span className="id">{p.id}</span>
              <span className="title">{p.title}</span>
              <span className="rating-cell">
                <Rating value={p.rating} />
              </span>
              <span className="tags">
                {p.tags?.map((t) => (
                  <span
                    key={t}
                    className="tag"
                    onClick={(e) => {
                      e.stopPropagation();
                      setTag(t);
                    }}
                  >
                    {t}
                  </span>
                ))}
              </span>
              <span>{p.has_editorial && <span className="dot" />}</span>
            </div>
          ))}
          {problems?.length === 0 &&
            (q || tag || tier != null ? (
              <div className="empty">没有符合条件的题目</div>
            ) : (
              <div className="empty">
                <div className="empty-actions">
                  <span>题库还是空的</span>
                  <button className="btn primary" onClick={() => navigate({ name: "setter", contest: null })}>
                    <Icon name="sparkle" /> 出第一道题
                  </button>
                </div>
              </div>
            ))}
        </div>
      </section>
      <aside className="inspector">{current ? <Inspector problem={current} onOpen={() => open(current)} /> : <div className="empty">题库还是空的</div>}</aside>
    </div>
  );
}

function Inspector({ problem, onOpen }: { problem: ProblemSummary; onOpen: () => void }) {
  const doc = useMemo(() => ({ kind: "problem" as const, id: problem.id }), [problem.id]);
  const [report, setReport] = useState<CompileReport>();
  useEffect(() => {
    let live = true;
    api.compile(doc).then((r) => live && !r.superseded && setReport(r as CompileReport), () => live && setReport(undefined));
    return () => {
      live = false;
    };
  }, [doc]);
  const scale = (380 / 595) * 1.333 * Math.min(devicePixelRatio, 2) * 1.2;

  return (
    <>
      <header>
        <h2>
          {problem.title} <Rating value={problem.rating} />
        </h2>
        <div className="meta">
          <span className="mono">{problem.id}</span>
          {problem.sources?.map((s) => (
            <span key={s.platform + s.id}>
              {s.platform} {s.id}
            </span>
          ))}
        </div>
        <div className="actions">
          <button className="btn primary" onClick={onOpen}>
            <Icon name="edit" /> 编辑
          </button>
          {problem.has_editorial && (
            <button className="btn outline" onClick={() => navigate({ name: "studio", kind: "problem", id: problem.id, editorial: true })}>
              题解
            </button>
          )}
        </div>
      </header>
      <div className="thumbs">
        {report?.rev
          ? report.pageSizes.map(([w, h], i) => (
              <img key={`${report.rev}:${i}`} src={api.pageUrl(doc, i, scale, report.rev)} style={{ aspectRatio: `${w} / ${h}` }} alt={`第 ${i + 1} 页`} loading="lazy" />
            ))
          : null}
      </div>
    </>
  );
}
