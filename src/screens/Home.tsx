import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { requestAdmin } from "../lib/admin";
import { driveIcon, driveName } from "../lib/drives";
import { formatBytes, formatCount, formatDuration, percent, timeAgo } from "../lib/format";
import { kindColor, sortedBreakdown } from "../lib/kinds";
import { useStore } from "../store";

export function Home() {
  const { t } = useTranslation();
  const drives = useStore((s) => s.drives);
  const scans = useStore((s) => s.scans);
  const scanning = useStore((s) => s.scanning);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const startScan = useStore((s) => s.startScan);
  const explore = useStore((s) => s.explore);

  const total = drives?.reduce((a, d) => a + d.totalBytes, 0) ?? 0;
  const used = drives?.reduce((a, d) => a + d.totalBytes - d.freeBytes, 0) ?? 0;

  return (
    <div className="page">
      <div className="phead">
        <div>
          <h2>{t("home.title")}</h2>
          {drives && (
            <p>
              {t("home.summary", { count: drives.length, used: formatBytes(used), total: formatBytes(total) })}
            </p>
          )}
        </div>
        <button
          className="btn"
          disabled={!!scanning}
          onClick={async () => {
            const dir = await open({ directory: true, title: t("home.pickFolder") });
            if (typeof dir === "string") startScan(dir);
          }}
        >
          <Icon name="folder" />
          {t("home.scanFolder")}
        </button>
      </div>

      {!isAdmin && (
        <div className="infobar">
          <span className="ib-ic">
            <Icon name="shield" />
          </span>
          <div>
            <b>{t("home.noAdminTitle")}</b> {t("home.noAdminBody")}
          </div>
          <button className="btn accent sm" onClick={requestAdmin}>
            {t("titlebar.runAsAdmin")}
          </button>
        </div>
      )}

      {!drives && <p className="muted">{t("home.loading")}</p>}
      {drives?.length === 0 && <p className="muted">{t("home.noDrives")}</p>}

      <div className="drives">
        {drives?.map((d) => {
          const usedBytes = d.totalBytes - d.freeBytes;
          const pct = Math.round(percent(usedBytes, d.totalBytes));
          const scan = scans[d.root];
          const busy = scanning?.root === d.root;
          return (
            <div key={d.letter} className="card drive">
              <div className="dhead">
                <span className="dic">
                  <Icon name={driveIcon(d)} />
                </span>
                <div>
                  <div className="dn">{driveName(d)}</div>
                  <div className="ds">
                    {d.fileSystem} · {t(`drive.kind.${d.kind}`)}
                  </div>
                </div>
                <span className={`pill ${pct > 85 ? "warn" : ""}`}>{t("drive.full", { pct })}</span>
              </div>
              <div className="dnums">
                <span className="big">{formatBytes(usedBytes)}</span>
                <span className="of">{t("drive.usedOf", { total: formatBytes(d.totalBytes) })}</span>
              </div>
              <div className="dbar" role="img" aria-label={t("drive.full", { pct })}>
                {scan ? (
                  sortedBreakdown(scan.breakdown).map(([k, v]) => (
                    <i key={k} title={`${t(`kind.${k}`)}: ${formatBytes(v)}`} style={{ ["--c" as string]: kindColor(k), width: `${percent(v, d.totalBytes)}%` }} />
                  ))
                ) : (
                  <i className={pct > 85 ? "warn" : ""} style={{ width: `${pct}%` }} />
                )}
              </div>
              {scan && (
                <div className="leg">
                  {sortedBreakdown(scan.breakdown)
                    .slice(0, 4)
                    .map(([k, v]) => (
                      <span key={k} style={{ ["--c" as string]: kindColor(k) }}>
                        <i />
                        {t(`kind.${k}`)} {formatBytes(v)}
                      </span>
                    ))}
                  <span className="faint">{t("drive.free", { size: formatBytes(d.freeBytes) })}</span>
                </div>
              )}
              <div className="dfoot">
                <span>
                  {scan
                    ? t("drive.scanned", {
                        files: formatCount(scan.files),
                        method: t(`scan.method.${scan.method}`),
                        elapsed: formatDuration(scan.elapsedMs),
                        ago: timeAgo(scan.scannedAt),
                      })
                    : d.supportsFastScan
                      ? t("drive.fastScan")
                      : t("drive.folderWalk", { fs: d.fileSystem })}
                </span>
                <div className="row">
                  <button className="btn sm" disabled={!!scanning} onClick={() => startScan(d.root)}>
                    {busy ? <Icon name="refresh" className="spin" /> : null}
                    {scan ? t("drive.rescan") : t("drive.scan")}
                  </button>
                  {scan && (
                    <button className="btn sm accent" onClick={() => explore(d.root)}>
                      {t("drive.explore")}
                    </button>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
