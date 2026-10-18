import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { api, KINDS, type FileHit, type Kind } from "../lib/api";
import { formatBytes, timeAgo } from "../lib/format";
import { KIND_ICON, kindColor } from "../lib/kinds";
import { useStore } from "../store";

const GB = 1024 ** 3;
const YEAR_MS = 365 * 24 * 3600 * 1000;
const key = (f: FileHit) => `${f.root}|${f.id}`;

export function Files() {
  const { t } = useTranslation();
  const scans = useStore((s) => s.scans);
  const showToast = useStore((s) => s.showToast);
  const [tab, setTab] = useState<"size" | "age">("size");
  const [bigOnly, setBigOnly] = useState(true);
  const [hideProtected, setHideProtected] = useState(false);
  const [kind, setKind] = useState<Kind | "all">("all");
  const [hits, setHits] = useState<FileHit[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const scanVersion = Object.values(scans)
    .map((s) => s.scannedAt)
    .join(",");

  useEffect(() => {
    if (!scanVersion) return;
    let live = true;
    api
      .findFiles({
        sort: tab,
        minSize: tab === "size" && bigOnly ? GB : undefined,
        olderThan: tab === "age" ? Date.now() - YEAR_MS : undefined,
        kinds: kind === "all" ? undefined : [kind],
        limit: 300,
      })
      .then((h) => live && setHits(h))
      .catch((e) => showToast(String(e), true));
    return () => {
      live = false;
    };
  }, [tab, bigOnly, kind, scanVersion, showToast]);

  const rows = useMemo(() => (hits ?? []).filter((f) => !(hideProtected && f.protected)), [hits, hideProtected]);
  const sel = rows.filter((f) => selected.has(key(f)));
  const selSize = sel.reduce((a, f) => a + f.size, 0);

  if (!scanVersion)
    return (
      <main className="main">
        <div className="empty">
          <span className="ei">
            <Icon name="bigfile" />
          </span>
          <h3>{t("files.title")}</h3>
          <p>{t("files.noScans")}</p>
        </div>
      </main>
    );

  const toggle = (f: FileHit) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(key(f))) n.delete(key(f));
      else n.add(key(f));
      return n;
    });

  return (
    <>
      <main className="main">
        <div className="page">
          <div className="phead">
            <div>
              <h2>{t("files.title")}</h2>
              <p>{tab === "size" ? t("files.largestHint") : t("files.oldestHint")}</p>
            </div>
            <div className="seg">
              <button className={tab === "size" ? "on" : ""} onClick={() => setTab("size")}>
                {t("files.largest")}
              </button>
              <button className={tab === "age" ? "on" : ""} onClick={() => setTab("age")}>
                {t("files.oldest")}
              </button>
            </div>
          </div>
          <div className="chips">
            {tab === "size" && (
              <button className={`fchip ${bigOnly ? "on" : ""}`} onClick={() => setBigOnly(!bigOnly)}>
                {bigOnly && <Icon name="check" />}
                {t("files.minSize")}
              </button>
            )}
            <select
              className={`fchip ${kind !== "all" ? "on" : ""}`}
              value={kind}
              aria-label={t("files.type")}
              onChange={(e) => setKind(e.target.value as Kind | "all")}
            >
              <option value="all">{t("files.allTypes")}</option>
              {KINDS.map((k) => (
                <option key={k} value={k}>
                  {t(`kind.${k}`)}
                </option>
              ))}
            </select>
            <button className={`fchip ${hideProtected ? "on" : ""}`} onClick={() => setHideProtected(!hideProtected)}>
              {hideProtected && <Icon name="check" />}
              {t("files.hideProtected")}
            </button>
          </div>
          <div className="tbl">
            <table>
              <thead>
                <tr>
                  <th style={{ width: 36 }} />
                  <th>{t("files.name")}</th>
                  <th className="r">{t("files.size")}</th>
                  <th>{t("files.modified")}</th>
                  <th>{t("files.type")}</th>
                </tr>
              </thead>
              <tbody>
                {hits && rows.length === 0 && (
                  <tr>
                    <td colSpan={5} className="muted">
                      {t("files.none")}
                    </td>
                  </tr>
                )}
                {rows.map((f) => (
                  <tr key={key(f)} className={selected.has(key(f)) ? "sel" : ""}>
                    <td>
                      <input
                        type="checkbox"
                        id={`f-${key(f)}`}
                        checked={selected.has(key(f))}
                        disabled={f.protected}
                        aria-label={f.name}
                        onChange={() => toggle(f)}
                      />
                    </td>
                    <td>
                      <div className="fname">
                        <span className="ftype" style={{ ["--c" as string]: kindColor(f.kind) }}>
                          <Icon name={KIND_ICON[f.kind]} />
                        </span>
                        <div>
                          <span className="nm selectable">
                            {f.name}{" "}
                            {f.protected && (
                              <span className="pill bad">
                                <Icon name="lock" />
                                {t("files.protected")}
                              </span>
                            )}
                          </span>
                          <span className="pt mono selectable">{f.folder}</span>
                        </div>
                      </div>
                    </td>
                    <td className="r num">{formatBytes(f.size)}</td>
                    <td className="num">
                      {f.modified > 0 ? new Date(f.modified).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : "—"}
                      {f.modified > 0 && <div className="faint" style={{ fontSize: 11.5 }}>{timeAgo(f.modified)}</div>}
                    </td>
                    <td className="muted">{t(`kind.${f.kind}`)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </main>
      <aside className="detail">
        <div className="dt">
          <div>
            <div className="faint">{t("files.selected")}</div>
            <div className="big">{formatBytes(selSize)}</div>
            <div className="muted">{t("files.filesCount", { count: sel.length })}</div>
          </div>
          {sel.length > 0 && (
            <div className="minilegend">
              {sel.slice(0, 12).map((f) => (
                <div key={key(f)} style={{ ["--c" as string]: kindColor(f.kind) }}>
                  <i />
                  <span>{f.name}</span>
                  <span>{formatBytes(f.size)}</span>
                </div>
              ))}
            </div>
          )}
          <div className="note">
            <Icon name="info" />
            <span>{t("files.binNote")}</span>
          </div>
        </div>
      </aside>
    </>
  );
}
