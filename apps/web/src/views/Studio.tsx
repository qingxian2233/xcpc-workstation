import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, api, type CompileReport, type ContestDetail, type Diagnostic, type DocRef, type ProblemDetail } from "../api";
import { Editor, type EditorHandle } from "../components/Editor";
import { Preview, type PreviewHandle } from "../components/Preview";
import { Samples } from "../components/Samples";
import { Icon } from "../components/Icon";
import { navigate } from "../lib/router";
import { useMedia, usePref, useWorkspaceEvents } from "../lib/hooks";

type Tab = "statement" | "samples" | "editorial";

interface OpenFile {
  path: string;
  /** Hash of the text on disk as we last saw it ("" = does not exist yet). */
  hash: string;
  /** Text on disk as we last saw it. */
  saved: string;
  /** Bumped when the file is (re)loaded from disk, to reset the editor. */
  gen: number;
}

type SaveState = "saved" | "dirty" | "saving" | "conflict" | "error";

interface Props {
  kind: "problem" | "contest";
  id: string;
  editorial: boolean;
  onPalette(): void;
}

export function Studio({ kind, id, editorial: initialEditorial, onPalette }: Props) {
  const isMobile = useMedia("(max-width: 820px)");
  const [mobileView, setMobileView] = useState<"edit" | "preview">("edit");
  const [vimOn, setVim] = usePref("vim", false);
  const [focus, setFocus] = useState(false);
  const [previewW, setPreviewW] = usePref("preview-width", 0);

  const [contest, setContest] = useState<ContestDetail>();
  const [problemId, setProblemId] = useState<string | undefined>(kind === "problem" ? id : undefined);
  const [problem, setProblem] = useState<ProblemDetail>();
  const [tab, setTab] = useState<Tab>(initialEditorial ? "editorial" : "statement");
  const [loadError, setLoadError] = useState<string>();

  const doc: DocRef = useMemo(() => ({ kind, id, editorial: tab === "editorial" }), [kind, id, tab]);

  // ---- Loading ------------------------------------------------------------------------------

  const loadContest = useCallback(() => {
    if (kind !== "contest") return;
    api.contest(id).then((c) => {
      setContest(c);
      setProblemId((p) => p ?? c.items.find((i) => i.problem)?.problem?.id);
    }, (e) => setLoadError(e.message));
  }, [kind, id]);
  useEffect(loadContest, [loadContest]);

  const loadProblem = useCallback(() => {
    if (!problemId) return;
    api.problem(problemId).then(setProblem, (e) => setLoadError(e.message));
  }, [problemId]);
  useEffect(loadProblem, [loadProblem]);

  // ---- The open file -------------------------------------------------------------------------

  const filePath = problem
    ? tab === "editorial"
      ? problem.editorial?.path ?? `${problem.dir}/editorial.md`
      : tab === "statement"
        ? problem.statements[0]?.path ?? `${problem.dir}/statement.md`
        : undefined
    : undefined;

  const [file, setFile] = useState<OpenFile>();
  const text = useRef("");
  const [save, setSave] = useState<SaveState>("saved");
  const genRef = useRef(0);

  const openFile = useCallback(async (path: string) => {
    let disk = { text: "", hash: "" };
    try {
      disk = await api.read(path);
    } catch (e) {
      if (!(e instanceof ApiError && e.code === "not_found")) throw e;
    }
    text.current = disk.text;
    setFile({ path, hash: disk.hash, saved: disk.text, gen: ++genRef.current });
    setSave("saved");
  }, []);

  useEffect(() => {
    if (filePath) openFile(filePath).catch((e) => setLoadError(e.message));
    else setFile(undefined);
  }, [filePath, openFile]);

  // ---- Compile (debounced, with the editor's text overlaid) ----------------------------------

  const [report, setReport] = useState<CompileReport>();
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [compileError, setCompileError] = useState<string>();
  const [stale, setStale] = useState(false);
  const compileTimer = useRef<number | undefined>(undefined);

  const compile = useCallback(async () => {
    const unsaved = file && text.current !== file.saved ? { path: file.path, text: text.current } : undefined;
    try {
      const r = await api.compile(doc, unsaved);
      if (r.superseded) return;
      // Font substitution is the installation's business, not the author's.
      setDiagnostics(r.diagnostics.filter((d) => !/^FounderType CJK font/.test(d.message)));
      if (r.pages != null) {
        setReport(r);
        setCompileError(undefined);
        setStale(false);
      } else {
        const first = r.diagnostics.find((d) => d.severity === "error");
        setCompileError(first ? first.message : "排版失败");
      }
    } catch (e) {
      setCompileError((e as Error).message);
    }
  }, [doc, file]);

  const scheduleCompile = useCallback(
    (ms = 220) => {
      clearTimeout(compileTimer.current);
      compileTimer.current = window.setTimeout(compile, ms);
    },
    [compile],
  );

  // A different book (statements ↔ editorial) starts from a blank preview.
  useEffect(() => setReport(undefined), [doc]);
  useEffect(() => {
    scheduleCompile(0);
  }, [doc, file?.gen, scheduleCompile]);

  // ---- Autosave --------------------------------------------------------------------------------

  const saveTimer = useRef<number | undefined>(undefined);
  const fileRef = useRef(file);
  fileRef.current = file;

  const doSave = useCallback(async () => {
    clearTimeout(saveTimer.current);
    const f = fileRef.current;
    if (!f || text.current === f.saved) return;
    const body = text.current;
    setSave("saving");
    try {
      const { hash } = await api.write(f.path, body, f.hash);
      setFile((cur) => (cur && cur.path === f.path ? { ...cur, hash, saved: body } : cur));
      setSave(text.current === body ? "saved" : "dirty");
    } catch (e) {
      setSave(e instanceof ApiError && e.code === "conflict" ? "conflict" : "error");
    }
  }, []);

  const onChange = useCallback(
    (t: string) => {
      text.current = t;
      setSave("dirty");
      setStale(true);
      scheduleCompile();
      clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(doSave, 800);
    },
    [scheduleCompile, doSave],
  );

  // Flush on leave.
  useEffect(() => () => void doSave(), [doSave, file?.path]);

  // ---- Changes made outside (git, AI agents, another editor) ---------------------------------

  useWorkspaceEvents(async (paths) => {
    const f = fileRef.current;
    if (f && (paths.includes(f.path) || paths[0] === "*")) {
      const disk = await api.read(f.path).catch(() => null);
      if (disk && disk.hash !== f.hash) {
        if (text.current === f.saved) await openFile(f.path);
        else setSave("conflict");
      }
    }
    if (kind === "contest") loadContest();
    loadProblem();
    scheduleCompile(60);
  });

  const resolveConflict = async (keep: "mine" | "theirs") => {
    const f = fileRef.current!;
    if (keep === "theirs") return openFile(f.path);
    const disk = await api.read(f.path).catch(() => ({ hash: "", text: "" }));
    setFile({ ...f, hash: disk.hash, saved: disk.text });
    fileRef.current = { ...f, hash: disk.hash, saved: disk.text };
    await doSave();
  };

  // ---- Navigation helpers -------------------------------------------------------------------

  const editor = useRef<EditorHandle>(null);
  const preview = useRef<PreviewHandle>(null);
  const pendingReveal = useRef<{ line: number; column: number } | undefined>(undefined);

  const jumpTo = (loc: { file: string; line: number; column: number }) => {
    if (isMobile) setMobileView("edit");
    if (loc.file === file?.path) return editor.current?.reveal(loc.line, loc.column);
    // Another problem of the contest, or the other file of this one.
    const m = /^problems\/([^/]+)\/(statement(?:\.\w+)?|editorial)\.md$/.exec(loc.file);
    if (!m) return;
    pendingReveal.current = { line: loc.line, column: loc.column };
    setProblemId(m[1]);
    setTab(m[2] === "editorial" ? "editorial" : "statement");
  };
  useEffect(() => {
    if (file && pendingReveal.current) {
      const r = pendingReveal.current;
      pendingReveal.current = undefined;
      requestAnimationFrame(() => editor.current?.reveal(r.line, r.column));
    }
  }, [file]);

  const pickProblem = (pid: string) => {
    void doSave();
    setProblemId(pid);
    if (tab === "samples") setTab("statement");
    const range = report?.problems.find((p) => p.id === pid);
    if (range) preview.current?.scrollToPage(range.first - 1);
  };

  // ---- Split resize ---------------------------------------------------------------------------

  const root = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  const startDrag = (e: React.PointerEvent) => {
    const el = root.current!;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setDragging(true);
    let w = previewW;
    const move = (ev: PointerEvent) => {
      const rect = el.getBoundingClientRect();
      w = Math.min(Math.max(rect.right - ev.clientX, 320), rect.width - 360);
      el.style.setProperty("--preview-w", `${w}px`);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setDragging(false);
      setPreviewW(w);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  // ---- Render -------------------------------------------------------------------------------------

  const title = kind === "contest" ? contest?.meta.name : problem?.meta.title;
  const subtitle = kind === "contest" && problem ? `${contest?.items.find((i) => i.problem?.id === problem.id)?.item.label ?? ""}. ${problem.meta.title}` : undefined;
  const fileDiagnostics = diagnostics.filter((d) => d.file === file?.path && d.line);
  const readOnly = !!contest?.meta.frozen;

  const style = {
    "--outline": kind === "contest" && !isMobile ? "236px" : "0px",
    ...(previewW ? { "--preview-w": `${previewW}px` } : {}),
  } as React.CSSProperties;

  return (
    <div className="shell">
      <header className="topbar">
        <button className="btn icon" title="返回" onClick={() => navigate(kind === "contest" ? "/contests" : "/")}>
          <Icon name="back" />
        </button>
        <div style={{ display: "flex", alignItems: "baseline", gap: 10, minWidth: 0 }}>
          <strong style={{ whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{title ?? id}</strong>
          {subtitle && <span className="muted hide-mobile" style={{ whiteSpace: "nowrap" }}>{subtitle}</span>}
        </div>
        <div className="spacer" />
        <div className="nav mobile-switch">
          <a href="#" aria-current={mobileView === "edit" ? "page" : undefined} onClick={(e) => (e.preventDefault(), setMobileView("edit"))}>
            编辑
          </a>
          <a href="#" aria-current={mobileView === "preview" ? "page" : undefined} onClick={(e) => (e.preventDefault(), setMobileView("preview"))}>
            预览
          </a>
        </div>
        <button className="btn icon hide-mobile" title="搜索 (Ctrl K)" onClick={onPalette}>
          <Icon name="search" />
        </button>
        <button className="btn icon hide-mobile" title="Vim 模式" aria-pressed={vimOn} onClick={() => setVim(!vimOn)}>
          <Icon name="vim" />
        </button>
        <button className="btn icon hide-mobile" title="专注" aria-pressed={focus} onClick={() => setFocus(!focus)}>
          <Icon name="focus" />
        </button>
        <div className="split-btn">
          <a className="btn primary" href={api.pdfUrl(doc)} target="_blank" rel="noreferrer">
            <Icon name="pdf" />
            <span className="hide-mobile">{tab === "editorial" ? "题解 PDF" : "导出 PDF"}</span>
          </a>
          <a className="btn primary" href={api.pdfUrl(doc, true)} title="下载">
            <Icon name="download" />
          </a>
        </div>
      </header>

      <div className="studio" ref={root} style={style} data-focus={focus} data-mobile-view={mobileView}>
        <nav className="contest-outline" aria-label="题目">
          {contest?.items.map(({ item, problem: p }) => {
            const range = report?.problems.find((r) => r.label === item.label);
            return (
              <button key={item.label} aria-current={p?.id === problemId} disabled={!p} onClick={() => p && pickProblem(p.id)}>
                <span className="label">{item.label}</span>
                <span className="t">{p?.title ?? `缺失：${item.id}`}</span>
                <span className="p">{range ? `p.${range.first}` : ""}</span>
              </button>
            );
          })}
          {kind === "contest" && !readOnly && (
            <button className="outline-add" onClick={() => navigate({ name: "setter", contest: id })}>
              <Icon name="sparkle" /> 出一道题
            </button>
          )}
        </nav>

        <section className="pane">
          <div className="tabs" role="tablist">
            {(["statement", "samples", "editorial"] as Tab[]).map((t) => (
              <button key={t} className="tab" role="tab" aria-selected={tab === t} onClick={() => (void doSave(), setTab(t))}>
                {{ statement: "题面", samples: "样例", editorial: "题解" }[t]}
              </button>
            ))}
            <span className="spacer" />
            {save === "conflict" ? (
              <span className="save-state" style={{ color: "var(--warn)", display: "flex", gap: 6, alignItems: "center" }}>
                文件已在别处修改
                <button className="btn outline" style={{ height: 24 }} onClick={() => resolveConflict("theirs")}>
                  载入
                </button>
                <button className="btn outline" style={{ height: 24 }} onClick={() => resolveConflict("mine")}>
                  覆盖
                </button>
              </span>
            ) : (
              <span className="save-state">{save === "error" ? "保存失败" : save === "saving" || save === "dirty" ? "…" : ""}</span>
            )}
          </div>

          {loadError ? (
            <div className="empty">{loadError}</div>
          ) : tab === "samples" ? (
            problem && <Samples problem={problem} readOnly={readOnly} onSaved={(s) => (setProblem({ ...problem, samples: s }), scheduleCompile(0))} />
          ) : file ? (
            <Editor
              ref={editor}
              docKey={`${file.path}#${file.gen}`}
              initial={file.saved}
              onChange={onChange}
              onSave={doSave}
              diagnostics={fileDiagnostics}
              vim={vimOn}
              readOnly={readOnly}
            />
          ) : (
            <div className="editor-host" />
          )}

          {diagnostics.length > 0 && (
            <div className="problems-bar">
              {diagnostics.map((d, i) => (
                <button key={i} onClick={() => d.file && d.line && jumpTo({ file: d.file, line: d.line, column: d.column ?? 1 })}>
                  <span className={`sev ${d.severity}`}>{d.severity === "error" ? "错误" : "提示"}</span>
                  <span>{d.message}</span>
                  {d.file && (
                    <span className="where">
                      {d.file.replace(/^problems\//, "")}
                      {d.line ? `:${d.line}` : ""}
                    </span>
                  )}
                </button>
              ))}
            </div>
          )}
        </section>

        <div className="gutter-handle" data-active={dragging} onPointerDown={startDrag} onDoubleClick={() => (setPreviewW(0), root.current?.style.removeProperty("--preview-w"))} />

        <Preview ref={preview} doc={doc} report={report} stale={stale} error={compileError} onLocate={jumpTo} />
      </div>
    </div>
  );
}
