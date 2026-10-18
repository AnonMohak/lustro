// Stroke icons on a 20×20 grid, shared with the mockup.
const PATHS = {
  home: "M3 9.5 10 4l7 5.5V16a1 1 0 0 1-1 1h-3.5v-4.5h-5V17H4a1 1 0 0 1-1-1z",
  map: "M3.5 3.5h13v13h-13z M10 3.5v13 M10 9.5h6.5 M3.5 12h6.5",
  file: "M5 2.5h6.5L15 6v11.5H5z M11.5 2.5V6H15",
  bigfile: "M5 2.5h6.5L15 6v11.5H5z M11.5 2.5V6H15 M7.5 10.5h5 M7.5 13.5h5",
  dupe: "M7 6.5h9.5v10.5H7z M4 13.5V3.5h9",
  clean: "M10 2.5l1.6 4.4L16 8.5l-4.4 1.6L10 14.5l-1.6-4.4L4 8.5l4.4-1.6z M15.5 13.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z",
  apps: "M3.5 3.5h5v5h-5z M11.5 3.5h5v5h-5z M3.5 11.5h5v5h-5z M11.5 11.5h5v5h-5z",
  history: "M3.5 10a6.5 6.5 0 1 0 1.9-4.6 M3.5 3.5v3h3 M10 6.5V10l2.5 1.5",
  settings: "M10 7.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z M10 2.5v2 M10 15.5v2 M2.5 10h2 M15.5 10h2 M4.7 4.7l1.4 1.4 M13.9 13.9l1.4 1.4 M4.7 15.3l1.4-1.4 M13.9 6.1l1.4-1.4",
  search: "M8.5 3.5a5 5 0 1 0 0 10 5 5 0 0 0 0-10z M12.2 12.2l4.3 4.3",
  shield: "M10 2.5 16 5v4.5c0 3.8-2.6 6.6-6 8-3.4-1.4-6-4.2-6-8V5z",
  drive: "M3 11.5h14v4H3z M3 11.5 5 4.5h10l2 7 M13.5 13.5h1",
  usb: "M7.5 2.5h5v6h-5z M6 8.5h8v6a3 3 0 0 1-3 3H9a3 3 0 0 1-3-3z M9 4.5v1.5 M11 4.5v1.5",
  folder: "M2.5 5.5a1 1 0 0 1 1-1h4l1.5 2h7.5a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1h-13a1 1 0 0 1-1-1z",
  trash: "M4 5.5h12 M8 5.5v-2h4v2 M5.5 5.5l.7 11h7.6l.7-11 M8.5 8.5v5 M11.5 8.5v5",
  refresh: "M16 10a6 6 0 1 1-1.8-4.3 M16 3.5V7h-3.5",
  chev: "M8 5.5 12.5 10 8 14.5",
  open: "M11 3.5h5.5V9 M16.5 3.5l-7 7 M14 11.5v5H3.5V6h5",
  copy: "M7 7h9.5v9.5H7z M4 13V3.5h9.5",
  lock: "M5.5 9h9v8h-9z M7.5 9V6.5a2.5 2.5 0 0 1 5 0V9",
  check: "M4.5 10.5 8 14l7.5-8",
  code: "M7 6 3 10l4 4 M13 6l4 4-4 4",
  globe: "M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14z M3 10h14 M10 3c2 2 3 4.5 3 7s-1 5-3 7c-2-2-3-4.5-3-7s1-5 3-7z",
  box: "M3.5 6.5 10 3l6.5 3.5v7L10 17l-6.5-3.5z M3.5 6.5 10 10l6.5-3.5 M10 10v7",
  windows: "M3 4.5l6-1v6H3z M10.5 3.3l6.5-.8v7h-6.5z M3 10.5h6v6l-6-1z M10.5 10.5H17v7l-6.5-.8z",
  info: "M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14z M10 9v4.5 M10 6.5v.2",
  warn: "M10 3 17.5 16.5h-15z M10 8v4 M10 14.2v.2",
  up: "M10 16V4 M5.5 8.5 10 4l4.5 4.5",
  x: "M5 5l10 10 M15 5 5 15",
  min: "M5 10h10",
  max: "M5.5 5.5h9v9h-9z",
  restore: "M7.5 5.5 4 9l3.5 3.5 M4 9h8a4 4 0 0 1 0 8h-2",
  export: "M10 3v9 M6.5 6.5 10 3l3.5 3.5 M4 11.5v5h12v-5",
  video: "M3 5.5h10v9H3z M13 8.5l4-2v7l-4-2z",
  image: "M3 4.5h14v11H3z M3 13l4-4 3 3 2-2 5 5 M12.5 7.5v.1",
  doc: "M5 2.5h6.5L15 6v11.5H5z M11.5 2.5V6H15 M7.5 10h5 M7.5 13h5",
  app: "M3.5 4.5h13v11h-13z M3.5 7.5h13",
  game: "M6 6.5h8a3.5 3.5 0 0 1 0 7c-1.2 0-1.8-1-4-1s-2.8 1-4 1a3.5 3.5 0 0 1 0-7z M6.5 9v2 M5.5 10h2 M13 9.5v.1 M14.5 10.5v.1",
  archive: "M3.5 4h13v3.5h-13z M4.5 7.5v8.5h11V7.5 M8.5 10.5h3",
  bell: "M5.5 13.5V9a4.5 4.5 0 0 1 9 0v4.5l1.5 2h-12z M8.5 17h3",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <svg className={className ? `i ${className}` : "i"} viewBox="0 0 20 20" aria-hidden="true">
      <path d={PATHS[name]} />
    </svg>
  );
}
