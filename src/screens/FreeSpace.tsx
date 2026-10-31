import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../components/Icon";
import { api, type CleanItem, type CleanRule, type DupeGroup, type FileHit } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import { type Screen, useStore } from "../store";

const YEAR_MS = 365 * 86_400_000;
const BIG_OLD = 100 * 1024 * 1024;

interface Win {
  id: string;
  icon: IconName;
  title: string;
  desc: string;
  bytes: number;
  /** The button: an action that frees the space, or a screen where the user picks. */
  action: { label: string; run: () => void } | { label: string; go: Screen };
}

/** The bytes of a clean rule that a run frees: its selected items, or all of it. */
const ruleBytes = (r: CleanRule) =>
  r.items.length > 0 ? r.items.filter((i) => i.selected).reduce((a, i) => a + i.bytes, 0) : r.bytes;

/** One screen with the biggest easy wins of all features, each with one button. */
export function FreeSpace() {
  const { t } = useTranslation();
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const devAgeDays = useStore((s) => s.devAgeDays);
  const showToast = useStore((s) => s.showToast);
  const openDialog = useStore((s) => s.openDialog);
  const go = useStore((s) => s.go);
  const [clean, setClean] = useState<CleanRule[] | null>(null);
  const [installers, setInstallers] = useState<CleanItem[]>([]);
  const [dupes, setDupes] = useState<DupeGroup[] | null>(null);
  const [old, setOld] = useState<FileHit[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    setClean(null);
    Promise.all([
      api.cleanScan(devAgeDays),
      api.installersScan().catch(() => [] as CleanItem[]),
      api.dupesResult().catch(() => null),
      api.findFiles({ sort: "size", minSize: BIG_OLD, olderThan: Date.now() - YEAR_MS, limit: 50 }).catch(() => []),
    ])
      .then(([c, i, d, o]) => {
        setClean(c);
        setInstallers(i);
        setDupes(d);
        setOld(o);
      })
      .catch((e) => showToast(String(e), true));
  }, [devAgeDays, showToast]);
  useEffect(load, [load]);

  /** Runs built-in clean rules (their selected items) after a confirmation. */
  const runClean = (rules: CleanRule[], title: string) =>
    openDialog({
      title,
      body: [t("free.cleanBody")],
      confirm: t("free.go"),
      onConfirm: async () => {
        setBusy(true);
        try {
          const items: Record<string, string[]> = {};
          for (const r of rules)
            if (r.items.length > 0) items[r.id] = r.items.filter((i) => i.selected).map((i) => i.path);
          const out = await api.cleanRun(
            rules.map((r) => r.id),
            items,
            devAgeDays,
          );
          showToast(t("free.freed", { size: formatBytes(out.reduce((a, r) => a + r.freedBytes, 0)) }));
        } catch (e) {
          showToast(String(e), true);
        }
        setBusy(false);
        load();
      },
    });

  if (!clean)
    return (
      <div className="empty">
        <span className="ei">
          <Icon name="clean" className="spin" />
        </span>
        <h3>{t("free.looking")}</h3>
      </div>
    );

  const usable = (r: CleanRule) => (!r.admin || isAdmin) && r.running.length === 0 && !r.unavailable && r.bytes > 0;
  const junk = clean.filter((r) => usable(r) && r.safe && !r.warn && r.id !== "app-caches" && r.id !== "recycle-bin");
  const caches = clean.filter((r) => r.id === "app-caches" && usable(r));
  const bin = clean.filter((r) => r.id === "recycle-bin" && usable(r));
  const oldInstallers = installers.filter((i) => i.selected);
  const exact = (dupes ?? []).filter((g) => g.kind === "exact" || g.kind === "folder");
  const dupeBytes = exact.reduce((a, g) => a + g.size * (g.files.length - 1), 0);
  const sum = (rs: CleanRule[]) => rs.reduce((a, r) => a + ruleBytes(r), 0);

  const wins: Win[] = [
    {
      id: "junk",
      icon: "clean",
      title: t("free.junk"),
      desc: t("free.junkDesc", { count: junk.length }),
      bytes: sum(junk),
      action: {
        label: t("free.clean"),
        run: () => runClean(junk, t("free.junkConfirm", { size: formatBytes(sum(junk)) })),
      },
    },
    {
      id: "caches",
      icon: "archive",
      title: t("free.caches"),
      desc: t("free.cachesDesc", { count: caches[0]?.items.filter((i) => i.selected).length ?? 0 }),
      bytes: sum(caches),
      action: {
        label: t("free.clear"),
        run: () => runClean(caches, t("free.cachesConfirm", { size: formatBytes(sum(caches)) })),
      },
    },
    {
      id: "bin",
      icon: "trash",
      title: t("free.bin"),
      desc: t("free.binDesc"),
      bytes: sum(bin),
      action: {
        label: t("free.empty"),
        run: () => runClean(bin, t("free.binConfirm", { size: formatBytes(sum(bin)) })),
      },
    },
    {
      id: "installers",
      icon: "app",
      title: t("free.installers"),
      desc: t("free.installersDesc", { count: oldInstallers.length }),
      bytes: oldInstallers.reduce((a, i) => a + i.bytes, 0),
      action: {
        label: t("free.move"),
        run: () =>
          openDialog({
            title: t("free.installersConfirm", { count: oldInstallers.length }),
            body: [t("free.installersBody")],
            confirm: t("free.move"),
            onConfirm: async () => {
              setBusy(true);
              try {
                const out = await api.installersClean();
                showToast(t("free.freed", { size: formatBytes(out.freedBytes) }));
              } catch (e) {
                showToast(String(e), true);
              }
              setBusy(false);
              load();
            },
          }),
      },
    },
    {
      id: "dupes",
      icon: "dupe",
      title: t("free.dupes"),
      desc: dupes ? t("free.dupesDesc", { count: exact.length }) : t("free.dupesNone"),
      bytes: dupeBytes,
      action: { label: dupes ? t("free.review") : t("free.find"), go: "dupes" },
    },
    {
      id: "old",
      icon: "bigfile",
      title: t("free.old"),
      desc: t("free.oldDesc", { count: old.length }),
      bytes: old.reduce((a, f) => a + f.size, 0),
      action: { label: t("free.review"), go: "files" },
    },
  ];
  // Wins with nothing to free do not show, except duplicates before the first search.
  const shown = wins.filter((w) => w.bytes > 0 || (w.id === "dupes" && !dupes)).sort((a, b) => b.bytes - a.bytes);
  const total = shown.reduce((a, w) => a + w.bytes, 0);

  return (
    <div className="page">
      <div className="phead">
        <div>
          <h2>{t("free.title")}</h2>
          <p>{total > 0 ? t("free.summary", { size: formatBytes(total) }) : t("free.nothing")}</p>
        </div>
        <button className="btn sm" disabled={busy} onClick={load}>
          <Icon name="refresh" />
          {t("free.again")}
        </button>
      </div>
      <div className="card">
        {shown.map((w) => (
          <div key={w.id} className="citem">
            <Icon name={w.icon} />
            <div className="cn">{w.title}</div>
            <div className="cs">{w.bytes > 0 ? formatBytes(w.bytes) : ""}</div>
            <div className="cd">
              {w.desc}{" "}
              <button
                className={`btn sm ${"run" in w.action ? "accent" : ""}`}
                disabled={busy}
                style={{ marginTop: 6 }}
                onClick={() => ("run" in w.action ? w.action.run() : go(w.action.go))}
              >
                {w.action.label}
              </button>
            </div>
          </div>
        ))}
      </div>
      <p className="faint" style={{ fontSize: 12 }}>
        {t("free.note", { n: formatCount(shown.length) })}
      </p>
    </div>
  );
}
