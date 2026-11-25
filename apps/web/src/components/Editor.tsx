import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Compartment, EditorState, RangeSetBuilder } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  hoverTooltip,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
import { HighlightStyle, bracketMatching, syntaxHighlighting, syntaxTree } from "@codemirror/language";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { type Diagnostic as CmDiagnostic, lintGutter, setDiagnostics } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";
import { vim } from "@replit/codemirror-vim";
import katex from "katex";
import type { Diagnostic } from "../api";

export interface EditorHandle {
  reveal(line: number, column?: number): void;
  focus(): void;
}

interface Props {
  /** Identity of the document: a new key replaces the editor state (and its undo history). */
  docKey: string;
  initial: string;
  onChange(text: string): void;
  onSave(): void;
  /** Diagnostics for this file (1-based lines). */
  diagnostics: Diagnostic[];
  vim: boolean;
  readOnly?: boolean;
}

const highlight = HighlightStyle.define([
  { tag: t.heading1, class: "cm-heading" },
  { tag: t.heading2, class: "cm-heading cm-heading-2" },
  { tag: [t.heading3, t.heading4], class: "cm-heading" },
  { tag: t.strong, fontWeight: "650" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.link, color: "var(--accent-ink)" },
  { tag: t.url, color: "var(--ink-3)" },
  { tag: t.monospace, fontFamily: "var(--mono)", fontSize: "0.92em" },
  { tag: [t.processingInstruction, t.meta], color: "var(--ink-3)" },
  { tag: t.quote, color: "var(--ink-2)", fontStyle: "italic" },
  { tag: t.list, color: "var(--ink-3)" },
]);

// ---- Math: $…$ and $$…$$ get their own look and a rendered preview on hover ----------------

interface MathSpan { from: number; to: number; tex: string; display: boolean }

function mathSpans(state: EditorState, from: number, to: number): MathSpan[] {
  const out: MathSpan[] = [];
  const text = state.doc.sliceString(from, to);
  const re = /\$\$([\s\S]+?)\$\$|(?<![\\$])\$(?!\s)((?:\\.|[^$\\\n])+?)(?<!\s)\$/g;
  for (let m; (m = re.exec(text)); ) {
    const start = from + m.index;
    // Skip anything inside code (inline or fenced).
    const node = syntaxTree(state).resolveInner(start + 1, 1);
    let inCode = false;
    for (let n: typeof node | null = node; n; n = n.parent) if (/Code|CodeBlock|FencedCode/.test(n.name)) inCode = true;
    if (inCode) continue;
    out.push({ from: start, to: start + m[0].length, tex: m[1] ?? m[2], display: m[1] != null });
  }
  return out;
}

const mathMark = Decoration.mark({ class: "cm-math" });
const delimMark = Decoration.mark({ class: "cm-math-delim" });

const mathHighlight = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) this.decorations = this.build(u.view);
    }
    build(view: EditorView) {
      const b = new RangeSetBuilder<Decoration>();
      for (const { from, to } of view.visibleRanges) {
        for (const s of mathSpans(view.state, from, to)) {
          const d = s.display ? 2 : 1;
          b.add(s.from, s.from + d, delimMark);
          b.add(s.from + d, s.to - d, mathMark);
          b.add(s.to - d, s.to, delimMark);
        }
      }
      return b.finish();
    }
  },
  { decorations: (v) => v.decorations },
);

// ---- Code: fenced blocks get a mono, tinted band; inline code a chip --------------------------

const codeLine = Decoration.line({ class: "cm-codeblock" });
const codeLineFirst = Decoration.line({ class: "cm-codeblock cm-codeblock-first" });
const codeLineLast = Decoration.line({ class: "cm-codeblock cm-codeblock-last" });
const inlineCode = Decoration.mark({ class: "cm-inline-code" });

const codeStyling = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = this.build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state)) this.decorations = this.build(u.view);
    }
    build(view: EditorView) {
      const b = new RangeSetBuilder<Decoration>();
      const doc = view.state.doc;
      for (const { from, to } of view.visibleRanges) {
        syntaxTree(view.state).iterate({
          from,
          to,
          enter: (node) => {
            if (node.name === "FencedCode") {
              const first = doc.lineAt(node.from).number;
              const last = doc.lineAt(node.to).number;
              for (let n = first; n <= last; n++) {
                const line = doc.line(n);
                if (line.from < from || line.from > to) continue;
                b.add(line.from, line.from, n === first ? codeLineFirst : n === last ? codeLineLast : codeLine);
              }
              return false;
            }
            if (node.name === "InlineCode") {
              b.add(node.from, node.to, inlineCode);
              return false;
            }
          },
        });
      }
      return b.finish();
    }
  },
  { decorations: (v) => v.decorations },
);

const mathTooltip = hoverTooltip((view, pos) => {
  const line = view.state.doc.lineAt(pos);
  const lo = Math.max(0, line.from - 2000);
  const hi = Math.min(view.state.doc.length, line.to + 2000);
  const span = mathSpans(view.state, lo, hi).find((s) => pos >= s.from && pos <= s.to);
  if (!span) return null;
  return {
    pos: span.from,
    end: span.to,
    above: true,
    create() {
      const dom = document.createElement("div");
      dom.className = "cm-math-tip";
      try {
        katex.render(span.tex, dom, { displayMode: span.display, throwOnError: true, strict: "ignore" });
      } catch (e) {
        dom.innerHTML = `<span class="err"></span>`;
        dom.firstElementChild!.textContent = (e as Error).message.replace(/^KaTeX parse error: /, "");
      }
      return { dom };
    },
  };
});

// ---- The editor --------------------------------------------------------------------------

export const Editor = forwardRef<EditorHandle, Props>(function Editor(props, ref) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | undefined>(undefined);
  const vimSlot = useRef(new Compartment());
  const roSlot = useRef(new Compartment());
  const latest = useRef(props);
  latest.current = props;

  useEffect(() => {
    const state = EditorState.create({
      doc: props.initial,
      extensions: [
        vimSlot.current.of(props.vim ? vim({ status: true }) : []),
        roSlot.current.of(EditorState.readOnly.of(!!props.readOnly)),
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        drawSelection(),
        rectangularSelection(),
        highlightActiveLine(),
        bracketMatching(),
        closeBrackets(),
        highlightSelectionMatches(),
        search({ top: true }),
        lintGutter(),
        markdown({ base: markdownLanguage }),
        syntaxHighlighting(highlight),
        codeStyling,
        mathHighlight,
        mathTooltip,
        EditorView.lineWrapping,
        keymap.of([
          { key: "Mod-s", preventDefault: true, run: () => (latest.current.onSave(), true) },
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...searchKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) latest.current.onChange(u.state.doc.toString());
        }),
      ],
    });
    const v = new EditorView({ state, parent: host.current! });
    view.current = v;
    return () => v.destroy();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.docKey]);

  useEffect(() => {
    view.current?.dispatch({ effects: vimSlot.current.reconfigure(props.vim ? vim({ status: true }) : []) });
  }, [props.vim]);

  useEffect(() => {
    view.current?.dispatch({ effects: roSlot.current.reconfigure(EditorState.readOnly.of(!!props.readOnly)) });
  }, [props.readOnly]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const doc = v.state.doc;
    const list: CmDiagnostic[] = [];
    for (const d of props.diagnostics) {
      if (!d.line || d.line > doc.lines) continue;
      const line = doc.line(d.line);
      const from = Math.min(line.from + Math.max((d.column ?? 1) - 1, 0), line.to);
      const to = from < line.to ? Math.min(line.to, from + Math.max(1, wordEnd(line.text, from - line.from))) : from;
      list.push({ from, to, severity: d.severity, message: d.message + (d.hints?.length ? `\n${d.hints.join("\n")}` : "") });
    }
    v.dispatch(setDiagnostics(v.state, list));
  }, [props.diagnostics, props.docKey]);

  useImperativeHandle(ref, () => ({
    reveal(line, column = 1) {
      const v = view.current;
      if (!v) return;
      const l = v.state.doc.line(Math.min(Math.max(line, 1), v.state.doc.lines));
      const pos = Math.min(l.from + column - 1, l.to);
      v.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
      v.focus();
    },
    focus() {
      view.current?.focus();
    },
  }));

  return <div className="editor-host" ref={host} />;
});

function wordEnd(text: string, at: number): number {
  const m = /^[^\s，。；：,.;:]+/.exec(text.slice(at));
  return m ? m[0].length : 1;
}
