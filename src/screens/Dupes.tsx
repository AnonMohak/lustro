import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { Select } from "../components/Select";
import { api, type DupeGroup, type DupeKind, type DupeProgress } from "../lib/api";
import { confirmDelete } from "../lib/deleteFlow";
import { formatBytes, formatCount, percent } from "../lib/format";
import { baseName, dirName } from "../lib/platform";
import { useStore } from "../store";

const SIZES = [
  [64 * 1024, "64 KB"],
  [1024 * 1024, "1 MB"],
  [10 * 1024 * 1024, "10 MB"],
  [100 * 1024 * 1024, "100 MB"],
] as const;

const KINDS: DupeKind[] = ["image", "video", "doc", "archive", "other"];

const fileName = (p: string) => baseName(p);
const folderOf = (p: string) => dirName(p);

/** The space that deleting the picked files of a group frees. */
const pickedBytes = (g: DupeGroup, picked: Set<number> | undefined) =>
  [...(picked ?? [])].reduce((a, k) => a + g.files[k].size, 0);

/** A list of folders with "Add folder" and a remove button for each one. */
function FolderList({
  label,
  empty,
  title,
  folders,
  onChange,
}: {
  label: string;
  empty: string;
  title: string;
  folders: string[];
  onChange: (f: string[]) => void;
}) {
  const { t } = useTranslation();
  const add = async () => {
    const dir = await open({ directory: true, title });
    if (typeof dir === "string" && !folders.includes(dir)) onChange([...folders, dir]);
  };
  return (
    <div className="row" style={{ justifyContent: "center", flexWrap: "wrap" }}>
      <span className="muted">{label}</span>
      {folders.length === 0 && <span className="faint">{empty}</span>}
      {folders.map((f) => (
        <span key={f} className="pill">
          <span className="mono">{f}</span>
          <button
            className="linkbtn"
            aria-label={t("dupes.removeFolder", { path: f })}
            onClick={() => onChange(folders.filter((x) => x !== f))}
          >
            <Icon name="x" />
          </button>
        </span>
      ))}
      <button className="btn sm ghost" onClick={add}>
        <Icon name="folder" />
        {t("dupes.addFolder")}
      </button>
    </div>
  );
}

/** Index of the copy to keep in a group for a keep rule. */
function keepIndex(g: DupeGroup, rule: "oldest" | "newest" | "shortest"): number {
  let best = 0;
  g.files.forEach((f, i) => {
    const b = g.files[best];
    if (
      rule === "oldest"
        ? f.modified < b.modified
        : rule === "newest"
          ? f.modified > b.modified
          : f.path.length < b.path.length
    )
      best = i;
  });
  return best;
}

export function Dupes() {
  const { t } = useTranslation();
  const scans = useStore((s) => s.scans);
  const showToast = useStore((s) => s.showToast);
  const [minSize, setMinSize] = useState<number>(1024 * 1024);
  const [appData, setAppData] = useState(false);
  const [mode, setMode] = useState<"exact" | "name">("exact");
  /** Empty: all types. */
  const [kinds, setKinds] = useState<Set<DupeKind>>(new Set());
  const [onlyIn, setOnlyIn] = useState<string[]>([]);
  const [notIn, setNotIn] = useState<string[]>([]);
  const [groups, setGroups] = useState<DupeGroup[] | null>(null);
  const [progress, setProgress] = useState<DupeProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [sel, setSel] = useState(0);
  /** Per group: indexes of the copies to delete. */
  const [del, setDel] = useState<Set<number>[]>([]);
  const hasScans = Object.keys(scans).length > 0;

  const apply = (list: DupeGroup[], rule: "oldest" | "newest" | "shortest" = "oldest") => {
    setGroups(list);
    // Lustro selects extra copies only of plain user files (media, documents, archives) outside
    // tool folders. Programs, libraries and app data stay unselected: something may need each copy.
    const userFile =
      /\.(mp4|mkv|mov|avi|wmv|webm|m4v|mp3|flac|wav|m4a|aac|ogg|jpe?g|png|gif|heic|webp|tiff?|raw|cr2|nef|arw|dng|psd|pdf|docx?|xlsx?|pptx?|txt|epub|zip|rar|7z|iso)$/i;
    const toolFolder =
      /[\\/](appdata|programdata|program files[^\\/]*|node_modules|\.local|\.cache|\.config|snap|flatpak|\.cargo|\.rustup|\.gradle|\.m2|\.nuget|site-packages|\.venv|venv|target|\.git)[\\/]/i;
    const unsafe = (p: string) => !userFile.test(p) || toolFolder.test(p);
    // Name groups can hold different versions of a file, so nothing is selected there.
    setDel(
      list.map((g) =>
        g.kind !== "exact" || g.files.some((f) => unsafe(f.path))
          ? new Set<number>()
          : new Set(g.files.map((_, i) => i).filter((i) => i !== keepIndex(g, rule))),
      ),
    );
    setSel(0);
  };

  useEffect(() => {
    api
      .dupesResult()
      .then((r) => r && apply(r))
      .catch(() => {});
    const subs = [
      listen<DupeProgress>("dupes-progress", (e) => setProgress(e.payload)),
      listen<DupeGroup[]>("dupes-done", (e) => {
        setRunning(false);
        apply(e.payload);
      }),
      listen<string>("dupes-error", () => {
        setRunning(false);
        showToast(t("dupes.cancelled"));
      }),
    ];
    return () => subs.forEach((p) => p.then((off) => off()));
  }, [showToast, t]);

  const start = () => {
    setRunning(true);
    setProgress(null);
    api.dupesStart({ minSize, includeAppData: appData, mode, kinds: [...kinds], onlyIn, notIn }).catch((e) => {
      setRunning(false);
      showToast(String(e), true);
    });
  };

  if (!hasScans || running || !groups) {
    return (
      <main className="main">
        <div className="empty" style={{ maxWidth: 520 }}>
          <span className="ei">
            <Icon name="dupe" className={running ? "spin" : undefined} />
          </span>
          <h3>{running ? t("dupes.searching") : t("dupes.title")}</h3>
          {running ? (
            <>
              {progress && progress.totalBytes > 0 && (
                <>
                  <div className="share" style={{ width: "100%" }}>
                    <div className="bar">
                      <i
                        style={{
                          width: `${percent(progress.hashedBytes, progress.totalBytes)}%`,
                          background: "var(--accent)",
                        }}
                      />
                    </div>
                  </div>
                  <p>
                    {t("dupes.hashed", {
                      done: formatBytes(progress.hashedBytes),
                      total: formatBytes(progress.totalBytes),
                    })}{" "}
                    · {t("dupes.groupsFound", { count: progress.groups })}
                  </p>
                </>
              )}
              <button className="btn" onClick={() => api.dupesCancel()}>
                {t("dupes.cancel")}
              </button>
            </>
          ) : hasScans ? (
            <>
              <div className="seg">
                <button className={mode === "exact" ? "on" : ""} onClick={() => setMode("exact")}>
                  {t("dupes.modeExact")}
                </button>
                <button className={mode === "name" ? "on" : ""} onClick={() => setMode("name")}>
                  {t("dupes.modeName")}
                </button>
              </div>
              <p>{mode === "exact" ? t("dupes.intro") : t("dupes.nameIntro")}</p>
              <div className="row" style={{ justifyContent: "center", flexWrap: "wrap" }}>
                <span className="muted">{t("dupes.types")}</span>
                {KINDS.map((k) => (
                  <button
                    key={k}
                    className={`fchip ${kinds.has(k) ? "on" : ""}`}
                    aria-pressed={kinds.has(k)}
                    onClick={() =>
                      setKinds((s) => {
                        const n = new Set(s);
                        if (n.has(k)) n.delete(k);
                        else n.add(k);
                        return n;
                      })
                    }
                  >
                    {t(`dupes.kinds.${k}`)}
                  </button>
                ))}
              </div>
              <FolderList
                label={t("dupes.onlyIn")}
                empty={t("dupes.anywhere")}
                title={t("dupes.pickOnlyIn")}
                folders={onlyIn}
                onChange={setOnlyIn}
              />
              <FolderList
                label={t("dupes.notIn")}
                empty={t("dupes.nothingSkipped")}
                title={t("dupes.pickNotIn")}
                folders={notIn}
                onChange={setNotIn}
              />
              <div className="row" style={{ justifyContent: "center" }}>
                <span className="muted">{t("dupes.minSize")}</span>
                <Select
                  id="dupeMin"
                  ariaLabel={t("dupes.minSize")}
                  value={minSize}
                  options={SIZES.map(([v, l]) => ({ value: v, label: l }))}
                  onChange={setMinSize}
                />
                <button className="btn accent" onClick={start}>
                  {t("dupes.find")}
                </button>
              </div>
              <label className="row" style={{ justifyContent: "center", cursor: "pointer" }}>
                <input
                  type="checkbox"
                  id="dupeAppData"
                  checked={appData}
                  onChange={(e) => setAppData(e.target.checked)}
                />
                <span className="muted">{t("dupes.includeAppData")}</span>
              </label>
            </>
          ) : (
            <p>{t("dupes.noScans")}</p>
          )}
        </div>
      </main>
    );
  }

  if (groups.length === 0)
    return (
      <main className="main">
        <div className="empty">
          <span className="ei">
            <Icon name="check" />
          </span>
          <p>{mode === "name" ? t("dupes.noneName") : t("dupes.none")}</p>
          <button className="btn" onClick={() => setGroups(null)}>
            {t("dupes.again")}
          </button>
        </div>
      </main>
    );

  const savings = groups.reduce((a, g, i) => a + pickedBytes(g, del[i]), 0);
  const nDel = del.reduce((a, s) => a + s.size, 0);
  // Exact copies keep at least one. Name groups can hold different files, so all may go.
  const allGone = del.some((s, i) => groups[i].kind === "exact" && s.size >= groups[i].files.length);
  const g = groups[sel];
  const gDel = del[sel] ?? new Set<number>();

  const setKeep = (rule: "oldest" | "newest" | "shortest") =>
    setDel((d) =>
      d.map((s, i) =>
        i === sel ? new Set(groups[i].files.map((_, k) => k).filter((k) => k !== keepIndex(groups[i], rule))) : s,
      ),
    );

  const runDelete = () => {
    const items = groups.flatMap((grp, i) =>
      [...(del[i] ?? [])].map((k) => ({
        root: grp.files[k].root,
        id: grp.files[k].id,
        name: fileName(grp.files[k].path),
        path: grp.files[k].path,
        size: grp.files[k].size,
      })),
    );
    confirmDelete(items, false, () => {
      // Remove the deleted copies and groups with one copy left.
      const left = groups
        .map((grp, i) => ({ ...grp, files: grp.files.filter((_, k) => !del[i]?.has(k)) }))
        .filter((grp) => grp.files.length > 1);
      apply(left);
    });
  };

  return (
    <>
      <section className="list">
        <div className="lhead">
          <div className="ttl">
            <h3>{t("dupes.title")}</h3>
            <span className="pill acc">{t("dupes.toFree", { size: formatBytes(savings) })}</span>
          </div>
          <button className="btn sm" onClick={() => setGroups(null)}>
            <Icon name="refresh" />
            {t("dupes.again")}
          </button>
        </div>
        <div className="lscroll">
          {groups.slice(0, 500).map((grp, i) => (
            <button key={grp.hash + i} className={`grow-row ${i === sel ? "sel" : ""}`} onClick={() => setSel(i)}>
              <span className="t1">
                <b>{fileName(grp.files[0].path)}</b>
                <span className="num muted">{formatBytes(pickedBytes(grp, del[i]))}</span>
              </span>
              <span className="t2">
                <span>
                  {grp.kind === "name"
                    ? t("dupes.nameFiles", { n: grp.files.length, size: formatBytes(grp.size) })
                    : t("dupes.copies", { n: grp.files.length, size: formatBytes(grp.size) })}
                </span>
                <span>{t("dupes.toDelete", { n: del[i]?.size ?? 0 })}</span>
              </span>
            </button>
          ))}
          {groups.length > 500 && (
            <div className="faint" style={{ padding: 10 }}>
              {t("dupes.more", { n: formatCount(groups.length - 500) })}
            </div>
          )}
        </div>
      </section>
      <main className="main">
        <div className="page">
          <div className="phead">
            <div style={{ minWidth: 0 }}>
              <h2 style={{ fontSize: 18, overflowWrap: "anywhere" }}>{fileName(g.files[0].path)}</h2>
              <p>
                {g.kind === "name"
                  ? t("dupes.nameFiles", { n: g.files.length, size: formatBytes(g.size) })
                  : t("dupes.copies", { n: g.files.length, size: formatBytes(g.size) })}
              </p>
            </div>
            <div className="row">
              <span className="faint">{t("dupes.keep")}</span>
              <div className="seg">
                <button onClick={() => setKeep("oldest")}>{t("dupes.keepOldest")}</button>
                <button onClick={() => setKeep("newest")}>{t("dupes.keepNewest")}</button>
                <button onClick={() => setKeep("shortest")}>{t("dupes.keepShortest")}</button>
              </div>
            </div>
          </div>
          <div className="card">
            {g.files.map((f, k) => (
              <div key={f.path} className="dfile">
                <input
                  type="checkbox"
                  id={`dupe-${sel}-${k}`}
                  checked={gDel.has(k)}
                  aria-label={f.path}
                  onChange={() =>
                    setDel((d) =>
                      d.map((s, i) => {
                        if (i !== sel) return s;
                        const n = new Set(s);
                        if (n.has(k)) n.delete(k);
                        else n.add(k);
                        return n;
                      }),
                    )
                  }
                />
                <div style={{ minWidth: 0 }}>
                  <div className="mono selectable" style={{ overflowWrap: "anywhere" }}>
                    {folderOf(f.path)}
                  </div>
                  <div className="faint" style={{ fontSize: 12 }}>
                    {g.kind === "name" && `${fileName(f.path)} · ${formatBytes(f.size)} · `}
                    {t("dupes.modified", {
                      date: f.modified
                        ? new Date(f.modified).toLocaleDateString("en-GB", {
                            day: "numeric",
                            month: "short",
                            year: "numeric",
                          })
                        : "—",
                    })}
                    {g.kind === "name" && (
                      <>
                        {" "}
                        <span className={`pill ${f.same ? "acc" : ""}`}>
                          {f.same ? t("dupes.sameContent", { n: f.same }) : t("dupes.otherContent")}
                        </span>
                      </>
                    )}
                  </div>
                </div>
                {gDel.has(k) ? (
                  <span className="pill bad">{t("dupes.delete")}</span>
                ) : (
                  <span className="pill good">{t("dupes.kept")}</span>
                )}
                <button
                  className="btn ghost sm"
                  aria-label={t("dupes.openFolder")}
                  onClick={() => api.revealPath(f.path).catch((e) => showToast(String(e), true))}
                >
                  <Icon name="open" />
                </button>
              </div>
            ))}
          </div>
          {g.kind === "exact" && gDel.size >= g.files.length && (
            <div className="note lock">
              <Icon name="warn" />
              <span>{t("dupes.keepOne")}</span>
            </div>
          )}
        </div>
      </main>
      <aside className="detail">
        <div className="dt">
          <div>
            <div className="faint">{t("dupes.spaceToFree")}</div>
            <div className="big">{formatBytes(savings)}</div>
            <div className="muted">{t("dupes.inGroups", { n: formatCount(nDel), g: formatCount(groups.length) })}</div>
          </div>
          <div className="note">
            <Icon name="info" />
            <span>{t("dupes.note")}</span>
          </div>
          <div className="note">
            <Icon name="shield" />
            <span>{t("dupes.autoNote")}</span>
          </div>
          <button className="btn accent block" disabled={allGone || nDel === 0} onClick={runDelete}>
            <Icon name="trash" />
            {t("dupes.recycleBtn", { n: formatCount(nDel) })}
          </button>
        </div>
      </aside>
    </>
  );
}
