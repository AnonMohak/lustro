import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { api, type InstalledApp } from "../lib/api";
import { formatBytes, timeAgo } from "../lib/format";
import { useStore } from "../store";

const date = (ms: number) => (ms > 0 ? new Date(ms).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : "—");

export function Apps() {
  const { t } = useTranslation();
  const showToast = useStore((s) => s.showToast);
  const openDialog = useStore((s) => s.openDialog);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const [apps, setApps] = useState<InstalledApp[] | null>(null);
  const [q, setQ] = useState("");
  const [src, setSrc] = useState<"all" | "desktop" | "store">("all");
  const [selId, setSelId] = useState<string | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    setApps(null);
    api
      .appsList()
      .then((list) => {
        setApps(list);
        setSelId((id) => id ?? list[0]?.id ?? null);
      })
      .catch((e) => showToast(String(e), true));
  }, [reload, showToast]);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (apps ?? []).filter(
      (a) => (src === "all" || a.source === src) && (!needle || `${a.name} ${a.publisher}`.toLowerCase().includes(needle)),
    );
  }, [apps, q, src]);

  if (!apps)
    return (
      <main className="main">
        <div className="empty">
          <span className="ei">
            <Icon name="apps" className="spin" />
          </span>
          <p>{t("apps.loading")}</p>
        </div>
      </main>
    );

  const sel = apps.find((a) => a.id === selId) ?? null;
  const total = apps.reduce((a, x) => a + (x.size ?? 0), 0);

  const uninstall = (a: InstalledApp) =>
    openDialog({
      title: t("apps.uninstallTitle", { name: a.name }),
      body: [a.source === "store" ? t("apps.uninstallStore") : t("apps.uninstallDesktop", { publisher: a.publisher || a.name })],
      confirm: t("apps.uninstall"),
      danger: a.source === "store",
      onConfirm: () =>
        api
          .appUninstall(a.id)
          .then(() => {
            showToast(a.source === "store" ? t("apps.removed", { name: a.name }) : t("apps.started", { name: a.name }));
            if (a.source === "store") setReload((r) => r + 1);
          })
          .catch((e) => showToast(String(e), true)),
    });

  return (
    <>
      <main className="main">
        <div className="page">
          <div className="phead">
            <div>
              <h2>{t("apps.title")}</h2>
              <p>{t("apps.summary", { n: apps.length, size: formatBytes(total) })}</p>
            </div>
            <div className="row">
              <input
                className="text"
                id="appSearch"
                placeholder={t("apps.search")}
                aria-label={t("apps.search")}
                value={q}
                onChange={(e) => setQ(e.target.value)}
                style={{ width: 200 }}
              />
              <div className="seg">
                {(["all", "desktop", "store"] as const).map((v) => (
                  <button key={v} className={src === v ? "on" : ""} onClick={() => setSrc(v)}>
                    {t(`apps.${v}`)}
                  </button>
                ))}
              </div>
              <button className="btn sm ghost" onClick={() => setReload((r) => r + 1)} aria-label={t("apps.refresh")}>
                <Icon name="refresh" />
              </button>
            </div>
          </div>
          <div className="tbl">
            <table>
              <thead>
                <tr>
                  <th>{t("apps.name")}</th>
                  <th style={{ width: 120 }}>{t("apps.source")}</th>
                  <th className="r" style={{ width: 90 }}>
                    {t("apps.size")}
                  </th>
                  <th style={{ width: 105 }}>{t("apps.installed")}</th>
                  <th style={{ width: 95 }}>{t("apps.lastUsed")}</th>
                  <th style={{ width: 104 }} />
                </tr>
              </thead>
              <tbody>
                {rows.length === 0 && (
                  <tr>
                    <td colSpan={6} className="muted">
                      {t("apps.none")}
                    </td>
                  </tr>
                )}
                {rows.map((a) => (
                  <tr key={a.id} className={a.id === selId ? "sel" : ""} onClick={() => setSelId(a.id)}>
                    <td>
                      <div className="fname">
                        <span className="ftype" style={{ ["--c" as string]: a.source === "store" ? "var(--c-game)" : "var(--c-app)" }}>
                          <Icon name="app" />
                        </span>
                        <div>
                          <span className="nm" title={a.name}>
                            {a.name}
                          </span>
                          <span className="pt">{a.publisher}</span>
                        </div>
                      </div>
                    </td>
                    <td className="muted">{a.source === "store" ? t("apps.sourceStore") : t("apps.sourceDesktop")}</td>
                    <td className="r num">{a.size != null ? formatBytes(a.size) : "—"}</td>
                    <td className="num">{date(a.installed)}</td>
                    <td className="muted">{a.lastUsed > 0 ? timeAgo(a.lastUsed) : <span className="faint">{t("apps.unknown")}</span>}</td>
                    <td className="r">
                      <button
                        className="btn sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          uninstall(a);
                        }}
                      >
                        {t("apps.uninstall")}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </main>
      <aside className="detail">
        {sel && (
          <div className="dt" style={{ ["--c" as string]: sel.source === "store" ? "var(--c-game)" : "var(--c-app)" }}>
            <div className="dt-hero">
              <span className="dt-ic">
                <Icon name="app" />
              </span>
              <div className="dt-name selectable">{sel.name}</div>
              <div className="faint">{sel.publisher}</div>
            </div>
            <div>
              <div className="big">{sel.size != null ? formatBytes(sel.size) : "—"}</div>
              {sel.size != null && <div className="faint">{sel.sizeFrom === "folder" ? t("apps.sizeFolder") : t("apps.sizeRegistry")}</div>}
            </div>
            <dl className="kv">
              <dt>{t("apps.source")}</dt>
              <dd>{sel.source === "store" ? t("apps.sourceStore") : t("apps.sourceDesktop")}</dd>
              {sel.version && (
                <>
                  <dt>{t("apps.version")}</dt>
                  <dd>{sel.version}</dd>
                </>
              )}
              <dt>{t("apps.installed")}</dt>
              <dd>{date(sel.installed)}</dd>
              <dt>{t("apps.lastUsed")}</dt>
              <dd>{sel.lastUsed > 0 ? timeAgo(sel.lastUsed) : t("apps.unknown")}</dd>
            </dl>
            {sel.location && <div className="mono faint selectable" style={{ overflowWrap: "anywhere" }}>{sel.location}</div>}
            <button className="btn accent block" onClick={() => uninstall(sel)}>
              {t("apps.uninstall")}
            </button>
            <div className="note">
              <Icon name="info" />
              <span>{t("apps.note")}</span>
            </div>
            {!isAdmin && (
              <div className="note">
                <Icon name="shield" />
                <span>{t("apps.lastUsedNote")}</span>
              </div>
            )}
          </div>
        )}
      </aside>
    </>
  );
}
