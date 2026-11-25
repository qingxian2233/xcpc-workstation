import { useSyncExternalStore } from "react";

// Routes: /  /contests  /settings  /new[/c/:contest]  /p/:id[/editorial]  /c/:id[/editorial]
export type Route =
  | { name: "library" }
  | { name: "contests" }
  | { name: "settings" }
  | { name: "setter"; contest: string | null }
  | { name: "studio"; kind: "problem" | "contest"; id: string; editorial: boolean }
  | { name: "welcome" };

export function parse(path: string): Route {
  const parts = path.split("/").filter(Boolean).map(decodeURIComponent);
  if (parts[0] === "contests") return { name: "contests" };
  if (parts[0] === "settings") return { name: "settings" };
  if (parts[0] === "new") return { name: "setter", contest: parts[1] === "c" && parts[2] ? parts[2] : null };
  if (parts[0] === "welcome") return { name: "welcome" };
  if ((parts[0] === "p" || parts[0] === "c") && parts[1])
    return { name: "studio", kind: parts[0] === "p" ? "problem" : "contest", id: parts[1], editorial: parts[2] === "editorial" };
  return { name: "library" };
}

export function href(r: Route): string {
  switch (r.name) {
    case "library":
      return "/";
    case "contests":
      return "/contests";
    case "settings":
      return "/settings";
    case "setter":
      return r.contest ? `/new/c/${encodeURIComponent(r.contest)}` : "/new";
    case "welcome":
      return "/welcome";
    case "studio":
      return `/${r.kind === "problem" ? "p" : "c"}/${encodeURIComponent(r.id)}${r.editorial ? "/editorial" : ""}`;
  }
}

const listeners = new Set<() => void>();
window.addEventListener("popstate", () => listeners.forEach((l) => l()));

export function navigate(r: Route | string, replace = false) {
  const url = typeof r === "string" ? r : href(r);
  if (url === location.pathname) return;
  // View transitions give the cross-fade between screens for free where supported.
  const go = () => {
    history[replace ? "replaceState" : "pushState"](null, "", url);
    listeners.forEach((l) => l());
  };
  const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
  if (doc.startViewTransition && !matchMedia("(prefers-reduced-motion: reduce)").matches) doc.startViewTransition(go);
  else go();
}

export function useRoute(): Route {
  const path = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => location.pathname,
  );
  return parse(path);
}
