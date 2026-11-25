import { useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  type Brief,
  type CompileReport,
  type ContestListItem,
  type DocRef,
  type Draft,
  type DraftResult,
  type Idea,
  type SetterEnv,
  type Tier,
  type TierId,
  type VerifyReport,
} from "../api";
import { Icon, Rating } from "../components/Icon";
import { Preview } from "../components/Preview";
import { MathText } from "../components/MathText";
import { navigate } from "../lib/router";
import { useJob } from "../lib/jobs";
import { useLoad } from "../lib/hooks";

// ---- State that survives leaving the page (model calls are slow and not free) ---------------

type Step = "brief" | "ideas" | "draft";
interface Saved {
  step: Step;
  /** A job still running on the server, to pick up again after leaving the page. */
  pending?: { kind: "ideas" | "draft"; id: string } | null;
  brief: Brief;
  contest: string | null;
  ideas: Idea[];
  result: DraftResult | null;
}
const KEY = "xcpc:setter";
const emptyBrief: Brief = { background: "", tier: "medium", tags: [], notes: "" };
const load = (contest: string | null): Saved => {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? "") as Saved;
    if (s && s.brief) return contest ? { ...s, contest } : s;
  } catch {
    /* fresh start */
  }
  return { step: "brief", brief: emptyBrief, contest, ideas: [], result: null };
};

const TIER_COLOR: Record<TierId, string> = { warmup: "newbie", easy: "pupil", medium: "expert", hard: "cm", challenge: "gm" };

const COMMON_TAGS: [string, string][] = [
  ["greedy", "贪心"], ["dp", "动态规划"], ["math", "数学"], ["number theory", "数论"], ["combinatorics", "组合"],
  ["graphs", "图论"], ["trees", "树"], ["shortest paths", "最短路"], ["dsu", "并查集"], ["data structures", "数据结构"],
  ["binary search", "二分"], ["two pointers", "双指针"], ["strings", "字符串"], ["constructive", "构造"], ["bitmasks", "位运算"],
  ["geometry", "计算几何"], ["games", "博弈"], ["probabilities", "概率"], ["divide and conquer", "分治"], ["implementation", "模拟"],
];

const STEPS: [Step, string][] = [["brief", "需求"], ["ideas", "构思"], ["draft", "成稿"]];

export function Setter({ contest: routeContest }: { contest: string | null }) {
  const [state, setState] = useState<Saved>(() => load(routeContest));
  const patch = (p: Partial<Saved>) => setState((s) => ({ ...s, ...p }));
  useEffect(() => localStorage.setItem(KEY, JSON.stringify(state)), [state]);

  const { data: env } = useLoad(() => api.setterEnv(), []);
  const { data: contests } = useLoad(() => api.contests(), []);
  const ideasJob = useJob<{ ideas: Idea[] }>();
  const draftJob = useJob<DraftResult>();
  const [error, setError] = useState<string>();

  const fail = (e: unknown) => {
    if ((e as Error).name !== "AbortError") setError((e as Error).message);
  };

  const askIdeas = async (resume?: string) => {
    setError(undefined);
    patch({ step: "ideas", ...(resume ? {} : { ideas: [] }) });
    try {
      const r = await ideasJob.run(
        () => (resume ? Promise.resolve({ job: resume }) : api.setterIdeas(state.brief)),
        (id) => patch({ pending: { kind: "ideas", id } }),
      );
      patch({ ideas: r.ideas, pending: null });
    } catch (e) {
      if ((e as Error).name !== "AbortError") patch({ pending: null });
      fail(e);
    }
  };

  const writeOut = async (start: () => Promise<{ job: string }>) => {
    setError(undefined);
    patch({ step: "draft" });
    try {
      const r = await draftJob.run(start, (id) => patch({ pending: { kind: "draft", id } }));
      // Each result gets its own identity so the review pane starts fresh from it.
      patch({ result: { ...r, stamp: Date.now() }, pending: null });
    } catch (e) {
      if ((e as Error).name !== "AbortError") patch({ pending: null });
      fail(e);
    }
  };

  // Pick up a job that was still running when the page was left.
  useEffect(() => {
    const p = state.pending;
    if (p?.kind === "ideas") askIdeas(p.id);
    else if (p?.kind === "draft") writeOut(() => Promise.resolve({ job: p.id }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const restart = () => {
    ideasJob.cancel();
    draftJob.cancel();
    setError(undefined);
    setState({ step: "brief", brief: { ...emptyBrief, tier: state.brief.tier }, contest: state.contest, ideas: [], result: null, pending: null });
  };

  const reachable = (s: Step) => s === "brief" || (s === "ideas" && (state.ideas.length > 0 || ideasJob.running)) || (s === "draft" && (!!state.result || draftJob.running));

  return (
    <div className="shell">
      <header className="topbar">
        <button className="btn icon" title="返回" onClick={() => navigate(state.contest ? { name: "studio", kind: "contest", id: state.contest, editorial: false } : "/")}>
          <Icon name="back" />
        </button>
        <strong className="setter-title">
          <Icon name="sparkle" /> 出题
        </strong>
        <div className="spacer" />
        <ol className="stepper" aria-label="步骤">
          {STEPS.map(([s, label], i) => (
            <li key={s}>
              <button aria-current={state.step === s ? "step" : undefined} disabled={!reachable(s)} onClick={() => patch({ step: s })}>
                <span className="n num">{i + 1}</span>
                {label}
              </button>
            </li>
          ))}
        </ol>
        <div className="spacer" />
        <button className="btn hide-mobile" onClick={restart} disabled={state.step === "brief" && !state.brief.notes && !state.brief.background}>
          重新开始
        </button>
      </header>

      <main className="setter" data-step={state.step}>
        {error && (
          <div className="setter-error" role="alert">
            <Icon name="alert" />
            <span>{error}</span>
            <button className="btn icon" aria-label="关闭" onClick={() => setError(undefined)}>
              <Icon name="close" />
            </button>
          </div>
        )}
        {state.step === "brief" && (
          <BriefForm
            env={env}
            brief={state.brief}
            contest={state.contest}
            contests={contests ?? []}
            onChange={(brief) => patch({ brief })}
            onContest={(contest) => patch({ contest })}
            onSubmit={() => askIdeas()}
          />
        )}
        {state.step === "ideas" && (
          <IdeasStep
            ideas={state.ideas}
            running={ideasJob.running}
            onAgain={() => askIdeas()}
            onBack={() => patch({ step: "brief" })}
            onPick={(idea) => writeOut(() => api.setterWrite(state.brief, idea))}
          />
        )}
        {state.step === "draft" &&
          (draftJob.running ? (
            <Working job={draftJob.job} hasCompiler={!!env?.compiler.ok} onCancel={() => (draftJob.cancel(), patch({ pending: null, step: state.result ? "draft" : "ideas" }))} />
          ) : !state.result ? (
            <div className="empty">
              <div className="empty-actions">
                <span>这次没有写成。</span>
                <button className="btn outline" onClick={() => patch({ step: state.ideas.length ? "ideas" : "brief" })}>
                  回到{state.ideas.length ? "构思" : "需求"}
                </button>
              </div>
            </div>
          ) : (
            <DraftStep
              key={state.result.stamp ?? 0}
              brief={state.brief}
              result={state.result}
              contest={state.contest}
              contests={contests ?? []}
              hasCompiler={!!env?.compiler.ok}
              onContest={(contest) => patch({ contest })}
              onRevise={(draft, instruction) => writeOut(() => api.setterRevise(state.brief, draft, instruction))}
              onVerify={(draft) => writeOut(() => api.setterVerify(state.brief, draft))}
              onSaved={(id) => {
                localStorage.removeItem(KEY);
                navigate(state.contest ? { name: "studio", kind: "contest", id: state.contest, editorial: false } : { name: "studio", kind: "problem", id, editorial: false });
              }}
            />
          ))}
      </main>
    </div>
  );
}

// ---- 1. Brief -----------------------------------------------------------------------------------

function BriefForm(props: {
  env?: SetterEnv;
  brief: Brief;
  contest: string | null;
  contests: ContestListItem[];
  onChange(b: Brief): void;
  onContest(c: string | null): void;
  onSubmit(): void;
}) {
  const { brief, env } = props;
  const set = (p: Partial<Brief>) => props.onChange({ ...brief, ...p });
  const ready = !!env?.llm.configured;
  const { data: library } = useLoad(() => api.problems(), []);
  const knownTags = useMemo(() => {
    const seen = new Map<string, string>(COMMON_TAGS);
    for (const p of library ?? []) for (const t of p.tags ?? []) if (!seen.has(t)) seen.set(t, "");
    return [...seen.entries()];
  }, [library]);

  return (
    <form
      className="brief"
      onSubmit={(e) => {
        e.preventDefault();
        if (ready) props.onSubmit();
      }}
      onKeyDown={(e) => {
        if ((e.metaKey || e.ctrlKey) && e.key === "Enter" && ready) props.onSubmit();
      }}
    >
      {env && !env.llm.configured && (
        <div className="notice">
          <Icon name="settings" />
          <span>还没有配置模型服务。出题需要一个 OpenAI 兼容的接口。</span>
          <button type="button" className="btn outline" onClick={() => navigate("/settings")}>
            去设置
          </button>
        </div>
      )}
      {env && !env.compiler.ok && (
        <div className="notice quiet">
          <Icon name="beaker" />
          <span>没有找到 C++ 编译器，生成的题目将不会自动验证。</span>
          <button type="button" className="btn outline" onClick={() => navigate("/settings")}>
            怎么安装
          </button>
        </div>
      )}

      <fieldset>
        <legend>难度</legend>
        <div className="tiers" role="radiogroup">
          {(env?.tiers ?? []).map((t: Tier) => (
            <button
              type="button"
              key={t.id}
              role="radio"
              aria-checked={brief.tier === t.id}
              className="tier"
              data-tier={TIER_COLOR[t.id]}
              onClick={() => set({ tier: t.id })}
            >
              <span className="tier-label">{t.label}</span>
              <span className="tier-range num">
                CF {t.min}
                {t.id === "challenge" ? "+" : `–${t.max}`}
              </span>
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset>
        <legend>考察方向</legend>
        <TagPicker value={brief.tags} options={knownTags} onChange={(tags) => set({ tags })} />
      </fieldset>

      <label className="block-field">
        <span>命题意图</span>
        <textarea
          rows={4}
          value={brief.notes}
          onChange={(e) => set({ notes: e.target.value })}
          placeholder={"想考什么、想卡掉什么做法。\n例如：考树上倍增；卡掉 O(n√n) 的分块；数据里要有链和菊花图。"}
        />
      </label>

      <label className="block-field">
        <span>
          题目背景 <em>可选</em>
        </span>
        <textarea rows={2} value={brief.background} onChange={(e) => set({ background: e.target.value })} placeholder="例如：校运会的接力赛。留空由模型拟定。" />
      </label>

      <div className="brief-foot">
        <label className="select-field">
          <span>存入</span>
          <select value={props.contest ?? ""} onChange={(e) => props.onContest(e.target.value || null)}>
            <option value="">题库</option>
            {props.contests
              .filter((c) => !c.meta.frozen)
              .map((c) => (
                <option key={c.id} value={c.id}>
                  {c.meta.name}
                </option>
              ))}
          </select>
        </label>
        <span className="spacer" />
        <span className="muted hide-mobile hint">{navigator.platform.includes("Mac") ? "⌘" : "Ctrl"} ↵</span>
        <button className="btn primary big" disabled={!ready}>
          <Icon name="sparkle" /> 构思方案
        </button>
      </div>
    </form>
  );
}

function TagPicker({ value, options, onChange }: { value: string[]; options: [string, string][]; onChange(v: string[]): void }) {
  const [text, setText] = useState("");
  const add = (t: string) => {
    const tag = t.trim().toLowerCase();
    if (tag && !value.includes(tag)) onChange([...value, tag]);
    setText("");
  };
  const shown = options.filter(([t, zh]) => !value.includes(t) && (!text || t.includes(text.toLowerCase()) || zh.includes(text))).slice(0, text ? 12 : 20);
  return (
    <div className="tag-picker">
      <div className="tag-input">
        {value.map((t) => (
          <button type="button" key={t} className="chip" aria-pressed onClick={() => onChange(value.filter((x) => x !== t))}>
            {t} <Icon name="close" size={12} />
          </button>
        ))}
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && text.trim()) {
              e.preventDefault();
              add(shown[0]?.[0] ?? text);
            } else if (e.key === "Backspace" && !text && value.length) onChange(value.slice(0, -1));
          }}
          placeholder={value.length ? "" : "输入或从下面选择"}
        />
      </div>
      <div className="tag-options">
        {shown.map(([t, zh]) => (
          <button type="button" key={t} className="chip" onClick={() => add(t)}>
            {t}
            {zh && <span className="zh">{zh}</span>}
          </button>
        ))}
      </div>
    </div>
  );
}

// ---- 2. Ideas -----------------------------------------------------------------------------------

function IdeasStep({ ideas, running, onAgain, onBack, onPick }: { ideas: Idea[]; running: boolean; onAgain(): void; onBack(): void; onPick(i: Idea): void }) {
  return (
    <div className="ideas">
      <div className="ideas-bar">
        <span className="muted">{running ? "正在构思…" : ideas.length ? `${ideas.length} 个方案，选一个写成完整的题` : "没有拿到方案，可以换一批再试"}</span>
        <span className="spacer" />
        <button className="btn" onClick={onBack}>
          改需求
        </button>
        <button className="btn outline" disabled={running} onClick={onAgain}>
          <Icon name="refresh" /> 换一批
        </button>
      </div>
      <div className="idea-grid">
        {running
          ? [0, 1, 2].map((i) => <div key={i} className="idea skeleton" style={{ animationDelay: `${i * 120}ms` }} />)
          : ideas.map((idea, i) => (
              <article key={i} className="idea">
                <header>
                  <h3>{idea.title}</h3>
                  <Rating value={idea.rating} />
                </header>
                <p className="summary"><MathText text={idea.summary} /></p>
                <dl>
                  <dt>做法</dt>
                  <dd><MathText text={idea.solution} /></dd>
                  <dt>复杂度</dt>
                  <dd><MathText text={idea.complexity} /></dd>
                  {idea.killed.length > 0 && (
                    <>
                      <dt>卡掉</dt>
                      <dd>
                        <ul>
                          {idea.killed.map((k, j) => (
                            <li key={j}>
                              <strong><MathText text={k.approach} /></strong>：<MathText text={k.how} />
                            </li>
                          ))}
                        </ul>
                      </dd>
                    </>
                  )}
                </dl>
                <button className="btn primary" onClick={() => onPick(idea)}>
                  写成题目 <Icon name="arrowRight" />
                </button>
              </article>
            ))}
      </div>
    </div>
  );
}

// ---- 3. Working ---------------------------------------------------------------------------------

function useElapsed(active: boolean) {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (!active) return;
    const start = Date.now();
    setT(0);
    const id = setInterval(() => setT(Math.floor((Date.now() - start) / 1000)), 1000);
    return () => clearInterval(id);
  }, [active]);
  return t;
}

function Working({ job, hasCompiler, onCancel }: { job: { stage: string; round: number; report: VerifyReport | null } | null; hasCompiler: boolean; onCancel(): void }) {
  const secs = useElapsed(true);
  const stage = job?.stage || "开始";
  const writing = !job?.report && stage !== "验证";
  return (
    <div className="working">
      <div className="working-card">
        <div className="working-head">
          <span className="spinner" />
          <strong>{stage}</strong>
          {job && job.round > 0 && <span className="badge">第 {job.round} 轮修复</span>}
          <span className="spacer" />
          <span className="muted num">
            {Math.floor(secs / 60)}:{String(secs % 60).padStart(2, "0")}
          </span>
        </div>
        <ol className="checks">
          <li data-status={writing ? "running" : "ok"}>
            <StatusDot status={writing ? "running" : "ok"} />
            <span>撰写题面、样例与代码</span>
          </li>
          {hasCompiler &&
            (job?.report?.checks ?? DEFAULT_CHECKS).map((c) => (
              <li key={c.key} data-status={c.status}>
                <StatusDot status={c.status} />
                <span>{c.label}</span>
                <span className="detail">{c.detail.split("\n")[0]}</span>
              </li>
            ))}
        </ol>
        <p className="muted small">推理模型写完整道题通常要一到三分钟。可以离开这个页面，回来时进度还在。</p>
        <button className="btn" onClick={onCancel}>
          取消
        </button>
      </div>
    </div>
  );
}

const DEFAULT_CHECKS = [
  { key: "compile", label: "编译", status: "pending" as const, detail: "" },
  { key: "samples", label: "样例", status: "pending" as const, detail: "" },
  { key: "stress", label: "对拍", status: "pending" as const, detail: "" },
  { key: "speed", label: "标程速度", status: "pending" as const, detail: "" },
];

function StatusDot({ status }: { status: string }) {
  if (status === "ok") return <span className="sdot ok"><Icon name="check" size={12} /></span>;
  if (status === "fail") return <span className="sdot fail"><Icon name="close" size={12} /></span>;
  if (status === "warn") return <span className="sdot warn">!</span>;
  if (status === "running") return <span className="sdot running" />;
  return <span className="sdot" />;
}

// ---- 3. Draft -----------------------------------------------------------------------------------

type DraftTab = "samples" | "code" | "notes";

function DraftStep(props: {
  brief: Brief;
  result: DraftResult;
  contest: string | null;
  contests: ContestListItem[];
  hasCompiler: boolean;
  onContest(c: string | null): void;
  onRevise(d: Draft, instruction: string): void;
  onVerify(d: Draft): void;
  onSaved(id: string): void;
}) {
  const { result } = props;
  const [draft, setDraft] = useState<Draft>(result.draft);
  const [tab, setTab] = useState<DraftTab>("samples");
  const [instruction, setInstruction] = useState("");
  const [saving, setSaving] = useState(false);
  const [confirmUnverified, setConfirmUnverified] = useState(false);
  const [error, setError] = useState<string>();
  const report = result.report;
  const edited = JSON.stringify(draft) !== JSON.stringify(result.draft);
  const preview = "error" in result.preview ? undefined : (result.preview as CompileReport);
  const previewError = "error" in result.preview ? result.preview.error.message : preview && preview.pages == null ? "排版失败" : undefined;
  const doc: DocRef = useMemo(() => ({ kind: "problem", id: draft.slug }), [draft.slug]);
  const passed = !props.hasCompiler || (report?.ok ?? false);

  const save = async () => {
    if (!passed && !confirmUnverified) return setConfirmUnverified(true);
    setSaving(true);
    setError(undefined);
    try {
      const { id } = await api.setterCommit(props.brief, draft, report, props.contest);
      props.onSaved(id);
    } catch (e) {
      setError((e as Error).message);
      setSaving(false);
    }
  };

  const contestName = props.contests.find((c) => c.id === props.contest)?.meta.name;

  return (
    <div className="draft">
      <section className="draft-side">
        <div className="draft-head">
          <input className="draft-title" value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} aria-label="题目名称" />
          <div className="draft-meta">
            <Rating value={draft.rating} />
            <label className="slug">
              <span>ID</span>
              <input size={Math.max(draft.slug.length, 8)} value={draft.slug} onChange={(e) => setDraft({ ...draft, slug: e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, "") })} spellCheck={false} />
            </label>
            <span className="muted num">
              {draft.limits.timeMs / 1000} 秒 · {draft.limits.memoryMb} MB
            </span>
          </div>
          {draft.tags.length > 0 && (
            <div className="draft-tags">
              {draft.tags.map((t) => (
                <span key={t} className="tag">
                  {t}
                </span>
              ))}
            </div>
          )}
        </div>

        {props.hasCompiler && report && <Checklist report={report} rounds={result.rounds} />}

        <div className="tabs" role="tablist">
          {(
            [
              ["samples", "样例"],
              ["code", "代码"],
              ["notes", "思路与题解"],
            ] as [DraftTab, string][]
          ).map(([t, label]) => (
            <button key={t} className="tab" role="tab" aria-selected={tab === t} onClick={() => setTab(t)}>
              {label}
            </button>
          ))}
        </div>
        <div className="draft-body">
          {tab === "samples" && <SamplesView draft={draft} report={report} />}
          {tab === "code" && <CodeView draft={draft} />}
          {tab === "notes" && <NotesView draft={draft} />}
        </div>

        <form
          className="revise"
          onSubmit={(e) => {
            e.preventDefault();
            if (instruction.trim()) props.onRevise(draft, instruction.trim());
          }}
        >
          <textarea
            rows={2}
            value={instruction}
            onChange={(e) => setInstruction(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing && instruction.trim()) {
                e.preventDefault();
                props.onRevise(draft, instruction.trim());
              }
            }}
            placeholder="想怎么改？例如：n 改到 10⁶；样例 2 换成一条链；再卡掉一个贪心"
          />
          <button className="btn icon primary" aria-label="修改" disabled={!instruction.trim()}>
            <Icon name="send" />
          </button>
        </form>

        <div className="commit-bar">
          <label className="select-field">
            <span>存入</span>
            <select value={props.contest ?? ""} onChange={(e) => (props.onContest(e.target.value || null), setConfirmUnverified(false))}>
              <option value="">题库</option>
              {props.contests
                .filter((c) => !c.meta.frozen)
                .map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.meta.name}
                  </option>
                ))}
            </select>
          </label>
          <span className="spacer" />
          {error && <span className="error-text">{error}</span>}
          {edited && props.hasCompiler && (
            <button className="btn outline" onClick={() => props.onVerify(draft)}>
              重新验证
            </button>
          )}
          <button className={`btn primary${confirmUnverified ? " danger" : ""}`} disabled={saving || !draft.slug} onClick={save}>
            {confirmUnverified ? "验证没通过，仍然保存" : contestName ? `加入「${contestName}」` : "存入题库"}
          </button>
        </div>
      </section>

      <Preview
        doc={doc}
        pageUrl={(p, sc, r) => api.setterPageUrl(p, sc, r)}
        report={preview && preview.pages != null ? preview : undefined}
        stale={false}
        error={previewError}
      />
    </div>
  );
}

function Checklist({ report, rounds }: { report: VerifyReport; rounds: number }) {
  const [open, setOpen] = useState(!report.ok);
  const bad = report.checks.filter((c) => c.status === "fail" || c.status === "warn").length;
  return (
    <div className={`checklist${report.ok ? " passed" : " failed"}`}>
      <button className="checklist-head" onClick={() => setOpen(!open)} aria-expanded={open}>
        <StatusDot status={report.ok ? (bad ? "warn" : "ok") : "fail"} />
        <strong>{report.ok ? (bad ? "通过验证，有提示" : "通过验证") : "验证没通过"}</strong>
        <span className="muted">
          {report.ok && report.stressRuns > 0 && `对拍 ${report.stressRuns} 组 · 最大数据 ${report.stdMaxMs} ms`}
          {!report.ok && rounds > 0 && `已自动修复 ${rounds} 轮`}
        </span>
        <span className="spacer" />
        <span className={`chev${open ? " open" : ""}`}>
          <Icon name="chevron" />
        </span>
      </button>
      {open && (
        <ol className="checks">
          {report.checks.map((c) => (
            <li key={c.key} data-status={c.status}>
              <StatusDot status={c.status} />
              <span>{c.label}</span>
              <span className="detail">{c.detail}</span>
            </li>
          ))}
          {report.counterexample && (
            <li className="counter">
              <div>
                <span className="eyebrow">反例输入</span>
                <pre>{report.counterexample.input}</pre>
              </div>
              <div>
                <span className="eyebrow">std</span>
                <pre>{report.counterexample.expected || "—"}</pre>
              </div>
              <div>
                <span className="eyebrow">{report.counterexample.who}</span>
                <pre>{report.counterexample.got || "—"}</pre>
              </div>
            </li>
          )}
        </ol>
      )}
    </div>
  );
}

function SamplesView({ draft, report }: { draft: Draft; report: VerifyReport | null }) {
  return (
    <div className="draft-samples">
      {draft.samples.map((s, i) => {
        const checked = report?.samples[i];
        return (
          <div key={i} className="draft-sample">
            <header>
              <span className="num">样例 {i + 1}</span>
              {checked && (checked.claimedMatches ? <span className="ok-text">与手算一致</span> : <span className="warn-text">已按标程更正</span>)}
            </header>
            <div className="io">
              <pre>{s.input}</pre>
              <pre>{checked?.output ?? s.output}</pre>
            </div>
            {s.explanation && <p><MathText text={s.explanation} /></p>}
          </div>
        );
      })}
    </div>
  );
}

function CodeView({ draft }: { draft: Draft }) {
  const files: [string, string, string][] = [
    ["std", "标程", draft.std],
    ["brute", "暴力", draft.brute],
    ["random", "生成器", draft.generator],
    ...draft.wrong.map((w): [string, string, string] => [w.name, w.approach, w.code]),
  ];
  const [cur, setCur] = useState(files[0][0]);
  const file = files.find((f) => f[0] === cur) ?? files[0];
  const copied = useRef<number>(undefined);
  const [flash, setFlash] = useState(false);
  return (
    <div className="code-view">
      <div className="code-tabs">
        {files.map(([name, label]) => (
          <button key={name} className="chip" aria-pressed={cur === name} title={label} onClick={() => setCur(name)}>
            {name}.cpp
          </button>
        ))}
      </div>
      <div className="code-meta">
        <span className="muted">{file[1]}</span>
        <button
          className="btn"
          onClick={() => {
            navigator.clipboard?.writeText(file[2]);
            setFlash(true);
            clearTimeout(copied.current);
            copied.current = window.setTimeout(() => setFlash(false), 1200);
          }}
        >
          {flash ? "已复制" : "复制"}
        </button>
      </div>
      <pre className="code">{file[2]}</pre>
    </div>
  );
}

function NotesView({ draft }: { draft: Draft }) {
  const i = draft.idea;
  return (
    <div className="notes-view">
      <dl>
        <dt>题意</dt>
        <dd><MathText text={i.summary} /></dd>
        <dt>做法</dt>
        <dd><MathText text={i.solution} /></dd>
        <dt>复杂度</dt>
        <dd><MathText text={i.complexity} /></dd>
        {draft.wrong.length > 0 && (
          <>
            <dt>要卡掉的做法</dt>
            <dd>
              <ul>
                {draft.wrong.map((w) => (
                  <li key={w.name}>
                    <code>{w.name}</code> <MathText text={w.approach} />
                  </li>
                ))}
              </ul>
            </dd>
          </>
        )}
        <dt>题解</dt>
        <dd className="prose"><MathText text={draft.editorial} /></dd>
      </dl>
    </div>
  );
}
