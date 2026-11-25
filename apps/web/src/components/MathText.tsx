import { Fragment, useMemo } from "react";
import katex from "katex";

/**
 * Plain text with `$…$` / `$$…$$` formulas rendered by KaTeX. Only for short previews
 * (sample explanations, notes); the statement itself is typeset by Typst.
 */
export function MathText({ text, className }: { text: string; className?: string }) {
  const parts = useMemo(() => {
    const out: { math?: string; display?: boolean; text?: string }[] = [];
    const re = /\$\$([\s\S]+?)\$\$|\$((?:\\.|[^$\\\n])+?)\$/g;
    let last = 0;
    for (let m; (m = re.exec(text)); ) {
      if (m.index > last) out.push({ text: text.slice(last, m.index) });
      out.push({ math: m[1] ?? m[2], display: m[1] != null });
      last = m.index + m[0].length;
    }
    if (last < text.length) out.push({ text: text.slice(last) });
    return out.map((p) => {
      if (p.math == null) return p;
      try {
        return { html: katex.renderToString(p.math, { displayMode: p.display, throwOnError: true, strict: "ignore" }) };
      } catch {
        return { text: `$${p.math}$` };
      }
    });
  }, [text]);
  return (
    <span className={className} style={{ whiteSpace: "pre-wrap" }}>
      {parts.map((p, i) => ("html" in p ? <span key={i} dangerouslySetInnerHTML={{ __html: p.html as string }} /> : <Fragment key={i}>{p.text}</Fragment>))}
    </span>
  );
}
