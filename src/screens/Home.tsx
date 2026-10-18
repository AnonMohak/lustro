import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { requestAdmin } from "../lib/admin";
import { driveIcon, driveName } from "../lib/drives";
import { formatBytes, percent } from "../lib/format";
import { useStore } from "../store";

export function Home() {
  const { t } = useTranslation();
  const drives = useStore((s) => s.drives);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const go = useStore((s) => s.go);

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
                <i className={pct > 85 ? "warn" : ""} style={{ width: `${pct}%` }} />
              </div>
              <div className="dfoot">
                <span>
                  {d.supportsFastScan ? t("drive.fastScan") : t("drive.folderWalk", { fs: d.fileSystem })}
                </span>
                <button className="btn sm accent" onClick={() => go("explore")}>
                  {t("drive.explore")}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
