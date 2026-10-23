import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { driveIcon, driveName } from "../lib/drives";
import { formatBytes, formatCount, percent } from "../lib/format";
import { isDriveRoot } from "../lib/platform";
import { rootLabel } from "../lib/scanText";
import { checkForUpdates } from "../lib/updates";
import { useStore, type Screen } from "../store";
import { Icon, type IconName } from "./Icon";

const NAV: [Screen, IconName][] = [
  ["home", "home"],
  ["explore", "map"],
  ["files", "bigfile"],
  ["dupes", "dupe"],
  ["clean", "clean"],
  ["apps", "apps"],
  ["history", "history"],
];

/** Progress of the running delete or clean run, then its result for a few seconds. */
function JobBlock() {
  const { t } = useTranslation();
  const job = useStore((s) => s.job);
  const cancelJobs = useStore((s) => s.cancelJobs);
  if (!job) return null;
  const f = job.finished;

  if (f) {
    const parts = [
      f.freed > 0 && t("job.freed", { size: formatBytes(f.freed) }),
      f.recycled > 0 && t("job.recycled", { size: formatBytes(f.recycled) }),
    ].filter(Boolean);
    const title = f.errors > 0 ? t("job.errors", { count: f.errors }) : t(f.stopped ? "job.stopped" : "job.done");
    return (
      <div className={`job ${f.errors > 0 ? "warn" : "ok"}`} role="status">
        <div className="jt">
          <Icon name={f.errors > 0 ? "warn" : "check"} />
          <span className="lbl">{title}</span>
        </div>
        {parts.length > 0 && <div className="jm">{parts.join(" · ")}</div>}
      </div>
    );
  }

  const pct = job.totalBytes > 0 ? percent(job.doneBytes, job.totalBytes) : percent(job.done, job.total);
  return (
    <div className="job run" role="status">
      <div className="jt">
        <Icon name={job.kind === "clean" ? "clean" : "trash"} />
        <span className="lbl">{job.stopping ? t("job.stopping") : t(`job.${job.kind}`)}</span>
        {!job.stopping && (
          <button className="stop" title={t("job.stop")} aria-label={t("job.stop")} onClick={cancelJobs}>
            <Icon name="x" />
          </button>
        )}
      </div>
      <div className="jbar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)}>
        <i style={{ width: `${pct}%` }} />
      </div>
      <div className="jm">
        <span>{t("job.items", { done: formatCount(job.done), total: formatCount(job.total) })}</span>
        {job.totalBytes > 0 && <span>{t("job.bytes", { done: formatBytes(job.doneBytes), total: formatBytes(job.totalBytes) })}</span>}
      </div>
      {job.current && (
        <div className="jc mono" title={job.current}>
          {rootLabel(job.current)}
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  const { t } = useTranslation();
  const screen = useStore((s) => s.screen);
  const go = useStore((s) => s.go);
  const drives = useStore((s) => s.drives);
  const loadDrives = useStore((s) => s.loadDrives);
  const explore = useStore((s) => s.explore);
  const forgetScan = useStore((s) => s.forgetScan);
  const version = useStore((s) => s.info?.version);
  const scans = useStore((s) => s.scans);
  const exploreRoot = useStore((s) => s.exploreRoot);
  const driveRoots = drives?.map((d) => d.root) ?? [];
  const folders = Object.keys(scans).filter((r) => !isDriveRoot(r, driveRoots));
  const favorites = useStore((s) => s.favorites);
  const loadFavorites = useStore((s) => s.loadFavorites);
  const setFavorite = useStore((s) => s.setFavorite);
  const openFavorite = useStore((s) => s.openFavorite);
  const treeVersion = useStore((s) => s.treeVersion);

  // Sizes and scan positions change with every scan and delete.
  useEffect(() => {
    loadFavorites().catch(() => {});
  }, [scans, treeVersion, loadFavorites]);

  return (
    <nav className="sidebar" aria-label="Main">
      <button className={`sbtn searchbtn ${screen === "search" ? "on" : ""}`} onClick={() => go("search")}>
        <Icon name="search" />
        <span className="lbl">{t("nav.search")}</span>
        <span className="kbd">Ctrl+K</span>
      </button>
      {NAV.map(([id, icon]) => (
        <button key={id} className={`sbtn ${screen === id ? "on" : ""}`} onClick={() => go(id)}>
          <Icon name={icon} />
          <span className="lbl">{t(`nav.${id}`)}</span>
        </button>
      ))}

      {favorites.length > 0 && (
        <>
          <div className="side-sec">
            <span>{t("nav.favorites")}</span>
          </div>
          {favorites.map((f) => (
            <button key={f.path} className="sbtn" title={f.path} onClick={() => openFavorite(f)}>
              <Icon name="star" className="fill" />
              <span className="lbl">{rootLabel(f.path)}</span>
              {f.hits[0] && <span className="badge">{formatBytes(f.hits[0].size)}</span>}
              <span
                className="forget"
                role="button"
                tabIndex={0}
                title={t("nav.unstar")}
                aria-label={t("nav.unstar")}
                onClick={(e) => {
                  e.stopPropagation();
                  setFavorite(f.path, false);
                }}
              >
                <Icon name="x" />
              </span>
            </button>
          ))}
        </>
      )}

      <div className="side-sec">
        <span>{t("nav.drives")}</span>
        <button
          className="btn ghost sm"
          style={{ height: 22, padding: "0 4px" }}
          aria-label={t("nav.refreshDrives")}
          onClick={() => loadDrives()}
        >
          <Icon name="refresh" />
        </button>
      </div>
      {drives?.map((d) => {
        const pct = percent(d.totalBytes - d.freeBytes, d.totalBytes);
        return (
          <button key={d.letter} className="sbtn drv" title={driveName(d)} onClick={() => explore(d.root)}>
            <Icon name={driveIcon(d)} />
            <span className="drive-meta">
              <span className="top">
                <span className="lbl">{driveName(d)}</span>
                <span className="free num">{t("drive.free", { size: formatBytes(d.freeBytes) })}</span>
              </span>
              <span className="minibar">
                <i className={pct > 85 ? "warn" : ""} style={{ width: `${pct}%` }} />
              </span>
            </span>
          </button>
        );
      })}

      {folders.length > 0 && (
        <>
          <div className="side-sec">
            <span>{t("nav.folders")}</span>
          </div>
          {folders.map((r) => (
            <button
              key={r}
              className={`sbtn ${screen === "explore" && exploreRoot === r ? "on" : ""}`}
              title={r}
              onClick={() => explore(r)}
            >
              <Icon name="folder" />
              <span className="lbl">{rootLabel(r)}</span>
              <span className="badge">{formatBytes(scans[r].totalBytes)}</span>
              <span
                className="forget"
                role="button"
                tabIndex={0}
                title={t("nav.forget")}
                aria-label={t("nav.forget")}
                onClick={(e) => {
                  e.stopPropagation();
                  forgetScan(r);
                }}
              >
                <Icon name="x" />
              </span>
            </button>
          ))}
        </>
      )}

      <div className="grow" />
      <JobBlock />
      <button className="sbtn" title={t("nav.checkUpdates")} onClick={() => checkForUpdates(true)}>
        <Icon name="refresh" />
        <span className="lbl">{t("nav.checkUpdates")}</span>
        {version && <span className="ver">v{version}</span>}
      </button>
      <button className={`sbtn ${screen === "settings" ? "on" : ""}`} onClick={() => go("settings")}>
        <Icon name="settings" />
        <span className="lbl">{t("nav.settings")}</span>
      </button>
    </nav>
  );
}
