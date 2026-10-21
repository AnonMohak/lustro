import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import i18n from "../i18n";
import { api, type HistoryEntry } from "../lib/api";
import { saveExport, toCsv } from "../lib/exporters";
import { formatBytes } from "../lib/format";
import { baseName, dirName } from "../lib/platform";
import { useStore } from "../store";

const PILL: Record<HistoryEntry["action"], string> = {
  recycle: "acc",
  cleaner: "good",
  permanent: "bad",
  blocked: "warn",
};

function when(ms: number): string {
  const d = new Date(ms);
  const today = new Date();
  const time = d.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === today.toDateString()) return i18n.t("time.today", { time });
  const y = new Date(today);
  y.setDate(today.getDate() - 1);
  if (d.toDateString() === y.toDateString()) return i18n.t("time.yesterdayAt", { time });
  return `${d.toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })}, ${time}`;
}

function parent(path: string): string {
  return dirName(path) || path;
}

export function History() {
  const { t } = useTranslation();
  const treeVersion = useStore((s) => s.treeVersion);
  const showToast = useStore((s) => s.showToast);
  const [list, setList] = useState<HistoryEntry[] | null>(null);
  const [reload, setReload] = useState(0);

  useEffect(() => {
    api
      .history()
      .then(setList)
      .catch((e) => showToast(String(e), true));
  }, [treeVersion, reload, showToast]);

  const freed = (list ?? []).filter((h) => h.ok && h.action !== "blocked").reduce((a, h) => a + h.size, 0);

  return (
    <div className="page">
      <div className="phead">
        <div>
          <h2>{t("history.title")}</h2>
          <p>{t("history.summary", { size: formatBytes(freed) })}</p>
        </div>
        <button
          className="btn sm"
          disabled={!list || list.length === 0}
          onClick={() =>
            list &&
            saveExport(
              "csv",
              "Lustro history.csv",
              toCsv(
                ["Time", "Action", "Path", "Size (bytes)", "Size", "Result"],
                list.map((h) => [new Date(h.time).toISOString(), h.action, h.path, h.size, formatBytes(h.size), h.ok ? "OK" : (h.error ?? "Failed")]),
              ),
            )
          }
        >
          <Icon name="export" />
          {t("export.csv")}
        </button>
      </div>
      {list && list.length === 0 ? (
        <div className="empty">
          <span className="ei">
            <Icon name="history" />
          </span>
          <p>{t("history.empty")}</p>
        </div>
      ) : (
        <div className="tbl">
          <table>
            <thead>
              <tr>
                <th style={{ width: 160 }}>{t("history.when")}</th>
                <th style={{ width: 120 }}>{t("history.action")}</th>
                <th>{t("history.item")}</th>
                <th className="r" style={{ width: 90 }}>
                  {t("history.size")}
                </th>
                <th className="r" style={{ width: 190 }} />
              </tr>
            </thead>
            <tbody>
              {(list ?? []).map((h) => (
                <tr key={h.id}>
                  <td className="num muted">{when(h.time)}</td>
                  <td>
                    <span className={`pill ${PILL[h.action]}`}>
                      {h.action === "blocked" && <Icon name="lock" />}
                      {t(`history.actions.${h.action}`)}
                    </span>
                  </td>
                  <td>
                    <div className="fname">
                      <div>
                        <span className="nm selectable" title={h.path}>
                          {baseName(h.path)}
                        </span>
                        <span className="pt mono selectable" title={h.error ?? parent(h.path)}>
                          {h.ok ? parent(h.path) : h.error}
                        </span>
                      </div>
                    </div>
                  </td>
                  <td className="r num">{formatBytes(h.size)}</td>
                  <td className="r">
                    {h.restored ? (
                      <span className="faint" style={{ fontSize: 12 }}>
                        {t("history.restored")}
                      </span>
                    ) : h.restorable ? (
                      <button
                        className="btn sm"
                        onClick={() =>
                          api
                            .restore(h.id)
                            .then(() => {
                              showToast(t("history.restoredToast", { name: baseName(h.path) }));
                              setReload((r) => r + 1);
                            })
                            .catch((e) => {
                              showToast(String(e), true);
                              setReload((r) => r + 1);
                            })
                        }
                      >
                        <Icon name="restore" />
                        {t("history.restore")}
                      </button>
                    ) : h.action === "recycle" && h.ok ? (
                      <span className="faint" style={{ fontSize: 12 }}>
                        {t("history.gone")}
                      </span>
                    ) : h.action === "blocked" ? (
                      <span className="faint" style={{ fontSize: 12 }}>
                        {t("history.blockedNote")}
                      </span>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
