import { useTranslation } from "react-i18next";
import { driveIcon, driveName } from "../lib/drives";
import { formatBytes, percent } from "../lib/format";
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
  const folders = Object.keys(scans).filter((r) => !/^[A-Z]:\\$/.test(r));

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
