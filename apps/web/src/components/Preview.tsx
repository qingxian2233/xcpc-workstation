import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState } from "react";
import { api, type CompileReport, type DocRef } from "../api";
import { Icon } from "./Icon";

const PX_PER_PT = 96 / 72;
const MIN_ZOOM = 0.25;
const MAX_ZOOM = 4;

export interface PreviewHandle {
  scrollToPage(page: number): void;
}

interface Props {
  doc: DocRef;
  /** Where page images come from; defaults to the document's own pages. */
  pageUrl?(page: number, scale: number, rev: number): string;
  report?: CompileReport;
  /** The editor has changes the pages do not show yet. */
  stale: boolean;
  error?: string;
  onLocate?(loc: { file: string; line: number; column: number }): void;
  onPage?(page: number): void;
}

/** Rendering resolution snaps to quarter-octaves so small zoom steps reuse cached pages. */
const snapScale = (s: number) => Math.pow(2, Math.ceil(Math.log2(s) * 4) / 4);

export const Preview = forwardRef<PreviewHandle, Props>(function Preview({ doc, pageUrl, report, stale, error, onLocate, onPage }, ref) {
  const scroller = useRef<HTMLDivElement>(null);
  const pagesEl = useRef<HTMLDivElement>(null);
  const [zoom, setZoom] = useState<number | "fit">("fit");
  const [width, setWidth] = useState(0);
  const [page, setPage] = useState(0);

  const sizes = report?.pageSizes ?? [];
  const maxW = sizes.reduce((m, [w]) => Math.max(m, w), 595);
  const fit = width > 0 ? Math.min(Math.max((width - 48) / (maxW * PX_PER_PT), MIN_ZOOM), 2) : 1;
  const z = zoom === "fit" ? fit : zoom;
  const scale = snapScale(z * PX_PER_PT * Math.min(devicePixelRatio || 1, 3));

  useLayoutEffect(() => {
    const el = scroller.current!;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);

  // Which page is under the middle of the viewport.
  const onScroll = useCallback(() => {
    const el = scroller.current!;
    const mid = el.scrollTop + el.clientHeight / 3;
    const kids = pagesEl.current?.children;
    if (!kids) return;
    let p = 0;
    for (let i = 0; i < kids.length; i++) if ((kids[i] as HTMLElement).offsetTop <= mid) p = i;
    setPage(p);
    onPage?.(p);
  }, [onPage]);

  const scrollToPage = useCallback((p: number) => {
    const kid = pagesEl.current?.children[p] as HTMLElement | undefined;
    if (kid) scroller.current!.scrollTo({ top: kid.offsetTop - 16, behavior: "smooth" });
  }, []);
  useImperativeHandle(ref, () => ({ scrollToPage }), [scrollToPage]);

  // ---- Zoom gestures: transform only while the gesture lasts, then one commit. -------------
  const gesture = useRef<{ factor: number; ox: number; oy: number; timer?: number } | null>(null);
  const commitZoom = useCallback(
    (next: number, anchor?: { x: number; y: number }) => {
      const el = scroller.current!;
      const target = Math.min(Math.max(next, MIN_ZOOM), MAX_ZOOM);
      const ratio = target / z;
      const ax = anchor?.x ?? el.clientWidth / 2;
      const ay = anchor?.y ?? 0;
      const top = (el.scrollTop + ay) * ratio - ay;
      const left = (el.scrollLeft + ax) * ratio - ax;
      setZoom(target);
      requestAnimationFrame(() => {
        el.scrollTop = top;
        el.scrollLeft = left;
      });
    },
    [z],
  );

  useEffect(() => {
    const el = scroller.current!;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      const g = (gesture.current ??= { factor: 1, ox: e.clientX - rect.left, oy: e.clientY - rect.top });
      g.factor = Math.min(Math.max(g.factor * Math.exp(-e.deltaY * 0.01), MIN_ZOOM / z), MAX_ZOOM / z);
      const pages = pagesEl.current!;
      pages.style.transformOrigin = `${g.ox + el.scrollLeft}px ${g.oy + el.scrollTop}px`;
      pages.style.transform = `scale(${g.factor})`;
      clearTimeout(g.timer);
      g.timer = window.setTimeout(() => {
        pages.style.transform = "";
        gesture.current = null;
        commitZoom(z * g.factor, { x: g.ox, y: g.oy });
      }, 140);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [z, commitZoom]);

  const click = async (e: React.MouseEvent, i: number) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const [w, h] = sizes[i];
    const x = ((e.clientX - rect.left) / rect.width) * w;
    const y = ((e.clientY - rect.top) / rect.height) * h;
    if (!onLocate) return;
    const loc = await api.locate(doc, i, x, y).catch(() => null);
    if (loc) onLocate(loc);
  };

  return (
    <div className="preview">
      <div className={`scroller${stale ? " stale" : ""}`} ref={scroller} onScroll={onScroll}>
        <div className="pages" ref={pagesEl}>
          {sizes.map(([w, h], i) => (
            <div key={i} className="page" style={{ width: w * PX_PER_PT * z, height: h * PX_PER_PT * z }} onClick={(e) => click(e, i)}>
              {report && <PageImage url={(pageUrl ?? ((p, sc, r) => api.pageUrl(doc, p, sc, r)))(i, scale, report.rev)} root={scroller} />}
              <span className="page-no">{i + 1}</span>
            </div>
          ))}
        </div>
      </div>
      {error && <div className="fail">{error}</div>}
      {sizes.length > 0 && (
        <div className="hud">
          <button className="btn icon" title="缩小" onClick={() => commitZoom(z / 1.2)}>
            <Icon name="minus" />
          </button>
          <button className="btn val num" title="适应宽度" onClick={() => setZoom("fit")}>
            {Math.round(z * 100)}%
          </button>
          <button className="btn icon" title="放大" onClick={() => commitZoom(z * 1.2)}>
            <Icon name="plus" />
          </button>
          <span className="sep" />
          <button className="btn icon" title="上一页" disabled={page === 0} onClick={() => scrollToPage(page - 1)}>
            <Icon name="left" />
          </button>
          <span className="val num">
            {page + 1} / {sizes.length}
          </span>
          <button className="btn icon" title="下一页" disabled={page >= sizes.length - 1} onClick={() => scrollToPage(page + 1)}>
            <Icon name="right" />
          </button>
        </div>
      )}
    </div>
  );
});

/**
 * Shows `url` once it is decoded, keeping the previous image until then: a recompile or a
 * zoom never flashes white. Loads only near the viewport.
 */
function PageImage({ url, root }: { url: string; root: React.RefObject<HTMLDivElement | null> }) {
  const el = useRef<HTMLImageElement>(null);
  const [shown, setShown] = useState<string>();
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const io = new IntersectionObserver(([e]) => setVisible(e.isIntersecting), { root: root.current, rootMargin: "100% 0px" });
    io.observe(el.current!.parentElement!);
    return () => io.disconnect();
  }, [root]);

  useEffect(() => {
    if (!visible || url === shown) return;
    let live = true;
    const img = new Image();
    img.src = url;
    img.decode().then(
      () => live && setShown(url),
      () => {},
    );
    return () => {
      live = false;
    };
  }, [url, visible, shown]);

  return <img ref={el} src={shown} alt="" draggable={false} style={{ opacity: shown ? 1 : 0 }} />;
}
