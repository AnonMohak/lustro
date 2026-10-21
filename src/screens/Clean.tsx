import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../components/Icon";
import { requestAdmin } from "../lib/admin";
import { api, type CleanRule } from "../lib/api";
import { formatBytes, formatCount, timeAgo } from "../lib/format";
import { sepOf } from "../lib/platform";
import { useStore } from "../store";

const GROUPS: { id: CleanRule["group"]; icon: IconName }[] = [
  { id: "sys", icon: "windows" },
  { id: "web", icon: "globe" },
  { id: "dev", icon: "code" },
  { id: "docker", icon: "box" },
  { id: "ide", icon: "app" },
];

/** The last three parts of a path, for example "…\\app\\web\\node_modules". */
function shortPath(path: string): string {
  const sep = sepOf();
  const parts = path.split(sep);
  return parts.length > 4 ? `…${sep}${parts.slice(-3).join(sep)}` : path;
}

function ageText(days: number | null, t: (k: string, o?: Record<string, unknown>) => string): string {
  if (days === null) return "";
  if (days === 0) return t("clean.today");
  return t("clean.changed", { ago: timeAgo(Date.now() - days * 86_400_000) });
}

export function Clean() {
  const { t } = useTranslation();
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const showToast = useStore((s) => s.showToast);
  const openDialog = useStore((s) => s.openDialog);
  const runJob = useStore((s) => s.runJob);
  const devAgeDays = useStore((s) => s.devAgeDays);
  const [rules, setRules] = useState<CleanRule[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  /** Picked item paths of rules that have items. */
  const [picks, setPicks] = useState<Record<string, Set<string>>>({});
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [group, setGroup] = useState<CleanRule["group"]>("sys");
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);

  const hasItems = (r: CleanRule) => r.items.length > 0;
  const usable = (r: CleanRule) => (!r.admin || isAdmin) && r.running.length === 0 && !r.unavailable && r.bytes > 0;

  useEffect(() => {
    let live = true;
    setRules(null);
    api
      .cleanScan(devAgeDays)
      .then((list) => {
        if (!live) return;
        setRules(list);
        setSelected(new Set(list.filter((r) => r.safe && r.bytes > 0 && !hasItems(r)).map((r) => r.id)));
        const p: Record<string, Set<string>> = {};
        for (const r of list) if (hasItems(r)) p[r.id] = new Set(r.safe ? r.items.filter((i) => i.selected).map((i) => i.path) : []);
        setPicks(p);
      })
      .catch((e) => showToast(String(e), true));
    return () => {
      live = false;
    };
  }, [reload, devAgeDays, showToast]);

  const groups = useMemo(() => GROUPS.filter((g) => rules?.some((r) => r.group === g.id)), [rules]);
  const ruleBytes = (r: CleanRule) => {
    if (!usable(r)) return 0;
    if (hasItems(r)) return r.items.filter((i) => picks[r.id]?.has(i.path)).reduce((a, i) => a + i.bytes, 0);
    return selected.has(r.id) ? r.bytes : 0;
  };
  const isOn = (r: CleanRule) => ruleBytes(r) > 0;
  const selBytes = (list: CleanRule[]) => list.reduce((a, r) => a + ruleBytes(r), 0);
  const total = rules ? selBytes(rules) : 0;
  const adminCount = rules?.filter((r) => r.admin).length ?? 0;

  const toggleRule = (r: CleanRule) => {
    if (hasItems(r)) {
      setPicks((p) => ({ ...p, [r.id]: isOn(r) ? new Set() : new Set(r.items.map((i) => i.path)) }));
      return;
    }
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(r.id)) n.delete(r.id);
      else n.add(r.id);
      return n;
    });
  };
  const toggleItem = (r: CleanRule, path: string) =>
    setPicks((p) => {
      const n = new Set(p[r.id]);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return { ...p, [r.id]: n };
    });

  const selectGroup = (safeOnly: boolean, items: CleanRule[]) => {
    setSelected((s) => {
      const n = new Set([...s].filter((id) => !items.some((r) => r.id === id)));
      if (safeOnly) items.filter((r) => r.safe && r.bytes > 0 && !hasItems(r)).forEach((r) => n.add(r.id));
      return n;
    });
    setPicks((p) => {
      const n = { ...p };
      for (const r of items.filter(hasItems)) n[r.id] = new Set(safeOnly && r.safe ? r.items.filter((i) => i.selected).map((i) => i.path) : []);
      return n;
    });
  };

  const run = () => {
    if (!rules) return;
    const chosen = rules.filter(isOn);
    const items: Record<string, string[]> = {};
    for (const r of chosen.filter(hasItems)) items[r.id] = [...(picks[r.id] ?? [])];
    openDialog({
      title: t("clean.confirmTitle", { size: formatBytes(total) }),
      body: [
        ...groups
          .map((g) => [g, selBytes(rules.filter((r) => r.group === g.id))] as const)
          .filter(([, v]) => v > 0)
          .map(([g, v]) => `${t(`clean.groups.${g.id}`)}: ${formatBytes(v)}`),
        t("clean.confirmBody"),
      ],
      confirm: t("clean.confirm"),
      // The sidebar block shows the progress and the result; a toast shows only problems.
      onConfirm: async () => {
        setBusy(true);
        await runJob({
          kind: "clean",
          items: chosen.length,
          bytes: total,
          run: async () => {
            const res = await api.cleanRun(
              chosen.map((r) => r.id),
              items,
              devAgeDays,
            );
            const freed = res.reduce((a, r) => a + r.freedBytes, 0);
            const inUse = res.reduce((a, r) => a + r.skippedFiles, 0);
            const skipped = res.filter((r) => r.skipped);
            if (skipped.length > 0) showToast(t("clean.skippedRule", { name: skipped[0].name, reason: skipped[0].skipped }), true);
            else if (inUse > 0) showToast(t("clean.doneSkipped", { size: formatBytes(freed), n: formatCount(inUse) }));
            return { freed, errors: skipped.length };
          },
        });
        setBusy(false);
        setReload((r) => r + 1);
      },
    });
  };

  if (!rules)
    return (
      <main className="main">
        <div className="empty">
          <span className="ei">
            <Icon name="clean" className="spin" />
          </span>
          <h3>{busy ? t("clean.running") : t("clean.measuring")}</h3>
          <p>{t("clean.measuringHint")}</p>
        </div>
      </main>
    );

  const items = rules.filter((r) => r.group === group);
  return (
    <>
      <section className="list">
        <div className="lhead">
          <div className="ttl">
            <h3>{t("clean.title")}</h3>
            <button className="btn ghost sm" onClick={() => setReload((r) => r + 1)}>
              <Icon name="refresh" />
              {t("clean.rescan")}
            </button>
          </div>
        </div>
        <div className="lscroll">
          {groups.map((g) => {
            const list = rules.filter((r) => r.group === g.id);
            return (
              <button key={g.id} className={`grow-row ${g.id === group ? "sel" : ""}`} onClick={() => setGroup(g.id)}>
                <span className="t1">
                  <b>
                    <Icon name={g.icon} />
                    {t(`clean.groups.${g.id}`)}
                  </b>
                  <span className="num">{formatBytes(selBytes(list))}</span>
                </span>
                <span className="t2">
                  <span>{t("clean.ofSelected", { sel: list.filter(isOn).length, total: list.length })}</span>
                  <span>{t("clean.found", { size: formatBytes(list.reduce((a, r) => a + r.bytes, 0)) })}</span>
                </span>
              </button>
            );
          })}
        </div>
      </section>
      <main className="main">
        <div className="page">
          <div className="phead">
            <div>
              <h2>{t(`clean.groups.${group}`)}</h2>
              <p>{t("clean.groupHint")}</p>
            </div>
            <div className="row">
              <button className="btn sm ghost" onClick={() => selectGroup(false, items)}>
                {t("clean.selectNone")}
              </button>
              <button className="btn sm" onClick={() => selectGroup(true, items)}>
                {t("clean.selectSafe")}
              </button>
            </div>
          </div>
          <div className="card">
            {items.map((r) => {
              const ok = usable(r);
              const expanded = open.has(r.id);
              return (
                <div key={r.id} className={`citem ${ok ? "" : "off"}`}>
                  <input
                    type="checkbox"
                    id={`rule-${r.id}`}
                    checked={isOn(r)}
                    disabled={!ok}
                    aria-label={r.name}
                    onChange={() => toggleRule(r)}
                  />
                  <label className="cn" htmlFor={`rule-${r.id}`}>
                    {r.name}
                    {r.admin && (
                      <span className={`pill ${isAdmin ? "good" : "warn"}`}>
                        <Icon name="shield" />
                        {isAdmin ? t("clean.admin") : t("clean.needsAdmin")}
                      </span>
                    )}
                    {r.running.map((p) => (
                      <span key={p} className="pill warn">
                        {t("clean.isOpen", { app: p.replace(/\.exe$/i, "") })}
                      </span>
                    ))}
                    {r.warn && (
                      <span className="pill warn">
                        <Icon name="warn" />
                        {t("clean.readFirst")}
                      </span>
                    )}
                    {r.service && <span className="cmd">{t("clean.pausesService", { service: r.service })}</span>}
                  </label>
                  <div className="cs">
                    {formatBytes(hasItems(r) && ok ? ruleBytes(r) || r.bytes : r.bytes)}
                    <div className="faint" style={{ fontSize: 11.5, fontWeight: 400, textAlign: "right" }}>
                      {hasItems(r)
                        ? t("clean.folders", { count: r.items.length })
                        : r.files > 0
                          ? t("clean.files", { count: r.files, n: formatCount(r.files) })
                          : ""}
                    </div>
                  </div>
                  <div className="cd">
                    {r.unavailable ?? r.desc}{" "}
                    {hasItems(r) && (
                      <button
                        className="linkbtn"
                        onClick={() =>
                          setOpen((s) => {
                            const n = new Set(s);
                            if (n.has(r.id)) n.delete(r.id);
                            else n.add(r.id);
                            return n;
                          })
                        }
                      >
                        {expanded ? t("clean.hideFolders") : t("clean.showFolders")}
                      </button>
                    )}{" "}
                    {r.admin && !isAdmin && (
                      <button className="linkbtn" onClick={requestAdmin}>
                        {t("titlebar.runAsAdmin")}
                      </button>
                    )}
                  </div>
                  {hasItems(r) && expanded && (
                    <div className="subs">
                      {r.items.map((i) => (
                        <label key={i.path} className="sub">
                          <input
                            type="checkbox"
                            checked={picks[r.id]?.has(i.path) ?? false}
                            disabled={!ok}
                            onChange={() => toggleItem(r, i.path)}
                          />
                          <span className="sub-name">
                            <span className="mono" title={i.path}>
                              {shortPath(i.path)}
                            </span>
                            <span className="faint">{ageText(i.ageDays, t)}</span>
                          </span>
                          <span className="num">{formatBytes(i.bytes)}</span>
                        </label>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </main>
      <aside className="detail">
        <div className="dt">
          <div>
            <div className="faint">{t("clean.selectedAll")}</div>
            <div className="big">{formatBytes(total)}</div>
          </div>
          <div className="minilegend">
            {groups.map((g) => (
              <div key={g.id} style={{ ["--c" as string]: "var(--accent)" }}>
                <i />
                <span>{t(`clean.groups.${g.id}`)}</span>
                <span>{formatBytes(selBytes(rules.filter((r) => r.group === g.id)))}</span>
              </div>
            ))}
          </div>
          <button className="btn accent block" disabled={total === 0 || busy} onClick={run}>
            <Icon name="clean" />
            {total === 0 ? t("clean.nothing") : t("clean.cleanBtn", { size: formatBytes(total) })}
          </button>
          <div className="note">
            <Icon name="info" />
            <span>{t("clean.permanentNote")}</span>
          </div>
          {!isAdmin && adminCount > 0 && (
            <div className="note warn">
              <Icon name="shield" />
              <span>
                {t("clean.adminNote", { count: adminCount })}{" "}
                <button className="linkbtn" onClick={requestAdmin}>
                  {t("titlebar.runAsAdmin")}
                </button>
              </span>
            </div>
          )}
        </div>
      </aside>
    </>
  );
}
