import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { api, KINDS, type Kind, type SearchHit, type SearchResult } from "../lib/api";
import { confirmDelete } from "../lib/deleteFlow";
import { filesCsv, saveExport } from "../lib/exporters";
import { formatBytes, formatCount } from "../lib/format";
import { KIND_ICON, kindColor } from "../lib/kinds";
import { useStore } from "../store";

const SIZES: [number, string][] = [
  [0, ""],
  [1024 * 1024, "1 MB"],
  [100 * 1024 * 1024, "100 MB"],
  [1024 ** 3, "1 GB"],
];
const key = (h: SearchHit) => `${h.root}|${h.id}`;

export function Search() {
  const { t } = useTranslation();
  const scans = useStore((s) => s.scans);
  const treeVersion = useStore((s) => s.treeVersion);
  const showToast = useStore((s) => s.showToast);
  const [text, setText] = useState("");
  const [folders, setFolders] = useState(true);
  const [minSize, setMinSize] = useState(0);
  const [kind, setKind] = useState<Kind | "all">("all");
  const [res, setRes] = useState<SearchResult | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const input = useRef<HTMLInputElement>(null);
  const hasScans = Object.keys(scans).length > 0;

  useEffect(() => input.current?.focus(), []);

  useEffect(() => {
    if (!text.trim()) {
      setRes(null);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      api
        .search({ text, folders, minSize, kinds: kind === "all" ? undefined : [kind], limit: 500 })
        .then((r) => live && setRes(r))
        .catch((e) => showToast(String(e), true));
    }, 150);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [text, folders, minSize, kind, treeVersion, showToast]);

  const hits = res?.hits ?? [];
  const sel = hits.filter((h) => selected.has(key(h)));

  return (
    <div className="page">
      <div className="phead">
        <div>
          <h2>{t("search.title")}</h2>
          <p>{hasScans ? t("search.hint") : t("search.noScans")}</p>
        </div>
      </div>
      <div className="row">
        <input
          ref={input}
          className="text"
          id="searchText"
          value={text}
          disabled={!hasScans}
          placeholder={t("search.placeholder")}
          aria-label={t("search.title")}
          onChange={(e) => setText(e.target.value)}
          style={{ flex: 1, minWidth: 220, height: 36, fontSize: 14 }}
        />
        <div className="chips">
          <select className={`fchip ${minSize ? "on" : ""}`} value={minSize} aria-label={t("search.size")} onChange={(e) => setMinSize(Number(e.target.value))}>
            {SIZES.map(([v, l]) => (
              <option key={v} value={v}>
                {v === 0 ? t("search.anySize") : `≥ ${l}`}
              </option>
            ))}
          </select>
          <select className={`fchip ${kind !== "all" ? "on" : ""}`} value={kind} aria-label={t("files.type")} onChange={(e) => setKind(e.target.value as Kind | "all")}>
            <option value="all">{t("search.anyType")}</option>
            {KINDS.map((k) => (
              <option key={k} value={k}>
                {t(`kind.${k}`)}
              </option>
            ))}
          </select>
          <button className={`fchip ${folders ? "on" : ""}`} onClick={() => setFolders(!folders)}>
            {folders && <Icon name="check" />}
            {t("search.folders")}
          </button>
        </div>
      </div>
      {res && (
        <div className="row faint num">
          <button
            className="btn sm"
            disabled={hits.length === 0}
            onClick={() =>
              saveExport(
                "csv",
                "Lustro search.csv",
                filesCsv(hits.map((h) => ({ name: h.name, path: h.path, size: h.size, modified: h.modified, kind: h.isDir ? "Folder" : t(`kind.${h.kind}`) }))),
              )
            }
          >
            <Icon name="export" />
            {t("export.csv")}
          </button>
          {t("search.info", {
            shown: formatCount(hits.length),
            total: formatCount(res.total),
            searched: formatCount(res.searched),
            ms: formatCount(res.elapsedMs),
          })}
        </div>
      )}
      {text.trim() && res && (
        <div className="tbl">
          <table>
            <thead>
              <tr>
                <th style={{ width: 40 }} />
                <th>{t("search.name")}</th>
                <th className="r" style={{ width: 90 }}>
                  {t("search.size")}
                </th>
                <th style={{ width: 120 }}>{t("search.modified")}</th>
                <th style={{ width: 52 }} />
              </tr>
            </thead>
            <tbody>
              {hits.length === 0 && (
                <tr>
                  <td colSpan={5} className="muted">
                    {t("search.none", { q: text })}
                  </td>
                </tr>
              )}
              {hits.map((h) => (
                <tr key={key(h)} className={selected.has(key(h)) ? "sel" : ""}>
                  <td>
                    <input
                      type="checkbox"
                      checked={selected.has(key(h))}
                      disabled={h.protected}
                      aria-label={h.name}
                      onChange={() =>
                        setSelected((s) => {
                          const n = new Set(s);
                          if (n.has(key(h))) n.delete(key(h));
                          else n.add(key(h));
                          return n;
                        })
                      }
                    />
                  </td>
                  <td>
                    <div className="fname">
                      <span className="ftype" style={{ ["--c" as string]: kindColor(h.kind) }}>
                        <Icon name={h.isDir ? "folder" : KIND_ICON[h.kind]} />
                      </span>
                      <div>
                        <span className="nm selectable">
                          {h.name}{" "}
                          {h.protected && (
                            <span className="pill bad">
                              <Icon name="lock" />
                              {t("files.protected")}
                            </span>
                          )}
                        </span>
                        <span className="pt mono selectable" title={h.folder}>
                          {h.folder}
                        </span>
                      </div>
                    </div>
                  </td>
                  <td className="r num">{formatBytes(h.size)}</td>
                  <td className="num">
                    {h.modified > 0 ? new Date(h.modified).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" }) : "—"}
                  </td>
                  <td className="r">
                    <button className="btn ghost sm" aria-label={t("explore.reveal")} onClick={() => api.revealPath(h.path).catch((e) => showToast(String(e), true))}>
                      <Icon name="open" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {sel.length > 0 && (
        <div className="actbar">
          <b>{t("del.selectedCount", { n: formatCount(sel.length) })}</b>
          <span className="muted num">{formatBytes(sel.reduce((a, h) => a + h.size, 0))}</span>
          <span className="grow" />
          <button className="btn sm ghost" onClick={() => setSelected(new Set())}>
            {t("del.clear")}
          </button>
          <button
            className="btn sm ghost danger"
            onClick={() => confirmDelete(sel.map((h) => ({ root: h.root, id: h.id, name: h.name, path: h.path, size: h.size })), true, () => setSelected(new Set()))}
          >
            {t("del.permanent")}
          </button>
          <button
            className="btn sm accent"
            onClick={() => confirmDelete(sel.map((h) => ({ root: h.root, id: h.id, name: h.name, path: h.path, size: h.size })), false, () => setSelected(new Set()))}
          >
            <Icon name="trash" />
            {t("del.recycle")}
          </button>
        </div>
      )}
    </div>
  );
}
