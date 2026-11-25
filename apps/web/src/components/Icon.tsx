// A handful of 16px stroke icons (drawn after Lucide, ISC).
const paths: Record<string, string> = {
  search: "M11 11l3.5 3.5M7 12.5a5.5 5.5 0 1 1 0-11 5.5 5.5 0 0 1 0 11z",
  back: "M10 3L5 8l5 5",
  chevron: "M4 6l4 4 4-4",
  left: "M10 4L6 8l4 4",
  right: "M6 4l4 4-4 4",
  plus: "M8 3v10M3 8h10",
  sparkle: "M8 1.5l1.4 3.6 3.6 1.4-3.6 1.4L8 11.5 6.6 7.9 3 6.5l3.6-1.4zM12.5 10.5l.6 1.4 1.4.6-1.4.6-.6 1.4-.6-1.4-1.4-.6 1.4-.6z",
  check: "M3 8.5l3.2 3L13 4.5",
  close: "M4 4l8 8M12 4l-8 8",
  alert: "M8 5.5v3.2M8 11h.01M7.1 2.5 1.8 12a1 1 0 0 0 .9 1.5h10.6a1 1 0 0 0 .9-1.5L8.9 2.5a1 1 0 0 0-1.8 0z",
  code: "M5.5 4.5 2 8l3.5 3.5M10.5 4.5 14 8l-3.5 3.5",
  beaker: "M6 1.5h4M6.5 1.5v4.2L2.6 12.6a1 1 0 0 0 .9 1.4h9a1 1 0 0 0 .9-1.4L9.5 5.7V1.5M4.5 9.5h7",
  arrowRight: "M3 8h10M9 4l4 4-4 4",
  send: "M14 2 7.5 14l-1.4-5.1L2 6.5 14 2ZM6.1 8.9 10 7",
  refresh: "M13 5V2l-2 2a5 5 0 1 0 1.4 7.8M13 5H9",
  settings: "M8 5.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5ZM13.5 8a5.5 5.5 0 0 0-.1-1l1.1-.9-1.2-2.1-1.4.5a5.2 5.2 0 0 0-1.7-1L10 2H6l-.2 1.5a5.2 5.2 0 0 0-1.7 1l-1.4-.5-1.2 2.1 1.1.9a5.5 5.5 0 0 0 0 2l-1.1.9 1.2 2.1 1.4-.5a5.2 5.2 0 0 0 1.7 1L6 14h4l.2-1.5a5.2 5.2 0 0 0 1.7-1l1.4.5 1.2-2.1-1.1-.9c.1-.3.1-.6.1-1Z",
  minus: "M3 8h10",
  pdf: "M9.5 1.5H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5zM9.5 1.5V5H13M5.5 9h5M5.5 11.5h3",
  download: "M8 2v8M4.5 7L8 10.5 11.5 7M3 13.5h10",
  edit: "M10.5 2.5l3 3L6 13H3v-3z",
  focus: "M2 5V3a1 1 0 0 1 1-1h2M11 2h2a1 1 0 0 1 1 1v2M14 11v2a1 1 0 0 1-1 1h-2M5 14H3a1 1 0 0 1-1-1v-2",
  panel: "M2.5 3h11a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zM10 3v10",
  trash: "M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.5 9h6l.5-9",
  fit: "M2 6V2h4M14 6V2h-4M2 10v4h4M14 10v4h-4",
  folder: "M2 4a1 1 0 0 1 1-1h3l1.5 1.5H13a1 1 0 0 1 1 1V12a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1z",
  book: "M3 2.5h7.5a2 2 0 0 1 2 2v9H5a2 2 0 0 1-2-2zM3 11.5a2 2 0 0 1 2-2h7.5",
  vim: "M2 3h4l2 6 2-6h4M5 3v10h2.5l5-10",
};

export function Icon({ name, size = 16 }: { name: keyof typeof paths | string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={paths[name]} />
    </svg>
  );
}

export function Brand() {
  return (
    <svg viewBox="0 0 64 64" aria-hidden>
      <rect width="64" height="64" rx="14" fill="var(--ink)" />
      <path d="M19 18h26v4H19zM19 28h26v2H19zM19 34h20v2H19zM19 40h23v2H19z" fill="var(--bg)" />
      <circle cx="46" cy="45" r="7" fill="var(--accent)" />
    </svg>
  );
}

const tiers: [number, string][] = [
  [2400, "gm"],
  [2100, "master"],
  [1900, "cm"],
  [1600, "expert"],
  [1400, "specialist"],
  [1200, "pupil"],
  [0, "newbie"],
];

export function Rating({ value }: { value?: number }) {
  if (value == null) return <span className="rating muted">—</span>;
  const tier = tiers.find(([min]) => value >= min)![1];
  return (
    <span className="rating" data-tier={tier}>
      {value}
    </span>
  );
}
