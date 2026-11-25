import { useEffect, useRef, useState } from "react";
import { api, type FuzzyHit } from "../api";
import { navigate } from "../lib/router";

export function CommandPalette({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [hits, setHits] = useState<FuzzyHit[]>([]);
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => input.current?.focus(), []);
  useEffect(() => {
    let live = true;
    api.search(text).then((h) => {
      if (!live) return;
      setHits(h);
      setSel(0);
    });
    return () => {
      live = false;
    };
  }, [text]);

  const go = (h: FuzzyHit) => {
    onClose();
    navigate({ name: "studio", kind: h.kind === "contest" ? "contest" : "problem", id: h.id, editorial: false });
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="palette" role="dialog" aria-label="跳转">
        <input
          ref={input}
          value={text}
          placeholder="题目、比赛、ID 或标签…"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            else if (e.key === "ArrowDown") (e.preventDefault(), setSel((s) => Math.min(s + 1, hits.length - 1)));
            else if (e.key === "ArrowUp") (e.preventDefault(), setSel((s) => Math.max(s - 1, 0)));
            else if (e.key === "Enter" && hits[sel]) go(hits[sel]);
          }}
        />
        {hits.length > 0 && (
          <ul role="listbox">
            {hits.map((h, i) => (
              <li key={`${h.kind}:${h.id}`} role="option" aria-selected={i === sel} onMouseMove={() => setSel(i)} onClick={() => go(h)}>
                <span className="kind">{h.kind === "contest" ? "比赛" : "题目"}</span>
                <span>{h.title}</span>
                <span className="hint">{h.id}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
