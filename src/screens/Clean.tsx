import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../components/Icon";
import { requestAdmin } from "../lib/admin";
import { api, type CleanRule } from "../lib/api";
import { formatBytes, formatCount } from "../lib/format";
import { useStore } from "../store";

const GROUPS: { id: CleanRule["group"]; icon: IconName }[] = [
  { id: "sys", icon: "windows" },
  { id: "web", icon: "globe" },
  { id: "dev", icon: "code" },
  { id: "docker", icon: "box" },
  { id: "ide", icon: "app" },
];

export function Clean() {
  const { t } = useTranslation();
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const showToast = useStore((s) => s.showToast);
  const openDialog = useStore((s) => s.openDialog);
  const [rules, setRules] = useState<CleanRule[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [group, setGroup] = useState<CleanRule["group"]>("sys");
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);

  const usable = (r: CleanRule) => (!r.admin || isAdmin) && r.running.length === 0;

  useEffect(() => {
    let live = true;
    setRules(null);
    api
      .cleanScan()
      .then((list) => {
        if (!live) return;
        setRules(list);
        setSelected(new Set(list.filter((r) => r.safe && r.bytes > 0).map((r) => r.id)));
      })
      .catch((e) => showToast(String(e), true));
    return () => {
      live = false;
    };
  }, [reload, showToast]);

  const groups = useMemo(() => GROUPS.filter((g) => rules?.some((r) => r.group === g.id)), [rules]);
  const isOn = (r: CleanRule) => selected.has(r.id) && usable(r);
  const selBytes = (list: CleanRule[]) => list.filter(isOn).reduce((a, r) => a + r.bytes, 0);
  const total = rules ? selBytes(rules) : 0;
  const adminCount = rules?.filter((r) => r.admin).length ?? 0;

  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const run = () => {
    if (!rules) return;
    const ids = rules.filter(isOn).map((r) => r.id);
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
      onConfirm: async () => {
        setBusy(true);
        try {
          const res = await api.cleanRun(ids);
          const freed = res.reduce((a, r) => a + r.freedBytes, 0);
          const inUse = res.reduce((a, r) => a + r.skippedFiles, 0);
          const skipped = res.filter((r) => r.skipped);
          if (skipped.length > 0) showToast(t("clean.skippedRule", { name: skipped[0].name, reason: skipped[0].skipped }), true);
          else if (inUse > 0) showToast(t("clean.doneSkipped", { size: formatBytes(freed), n: formatCount(inUse) }));
          else showToast(t("clean.doneToast", { size: formatBytes(freed) }));
        } catch (e) {
          showToast(String(e), true);
        } finally {
          setBusy(false);
          setReload((r) => r + 1);
        }
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
              <button
                className="btn sm ghost"
                onClick={() => setSelected((s) => new Set([...s].filter((id) => !items.some((r) => r.id === id))))}
              >
                {t("clean.selectNone")}
              </button>
              <button
                className="btn sm"
                onClick={() =>
                  setSelected((s) => {
                    const n = new Set([...s].filter((id) => !items.some((r) => r.id === id)));
                    items.filter((r) => r.safe && r.bytes > 0).forEach((r) => n.add(r.id));
                    return n;
                  })
                }
              >
                {t("clean.selectSafe")}
              </button>
            </div>
          </div>
          <div className="card">
            {items.map((r) => {
              const ok = usable(r);
              return (
                <div key={r.id} className={`citem ${ok ? "" : "off"}`}>
                  <input
                    type="checkbox"
                    id={`rule-${r.id}`}
                    checked={isOn(r)}
                    disabled={!ok || r.bytes === 0}
                    aria-label={r.name}
                    onChange={() => toggle(r.id)}
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
                    {formatBytes(r.bytes)}
                    <div className="faint" style={{ fontSize: 11.5, fontWeight: 400, textAlign: "right" }}>
                      {t("clean.files", { count: r.files, n: formatCount(r.files) })}
                    </div>
                  </div>
                  <div className="cd">
                    {r.desc}{" "}
                    {r.admin && !isAdmin && (
                      <button className="linkbtn" onClick={requestAdmin}>
                        {t("titlebar.runAsAdmin")}
                      </button>
                    )}
                  </div>
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
