import { useTranslation } from "react-i18next";
import { formatBytes } from "../lib/format";
import { useStore } from "../store";

export function StatusBar() {
  const { t } = useTranslation();
  const info = useStore((s) => s.info);
  const drives = useStore((s) => s.drives);
  const free = drives?.reduce((a, d) => a + d.freeBytes, 0) ?? 0;
  const anyLive = useStore((s) => Object.values(s.scans).some((x) => x.live));

  return (
    <footer className="status">
      <span className={`dot ${info?.isAdmin ? "live" : ""}`} />
      <span>{info?.isAdmin ? t("status.admin") : t("status.standard")}</span>
      {drives && (
        <>
          <span className="sep">·</span>
          <span className="num">{t("status.drives", { count: drives.length, free: formatBytes(free) })}</span>
        </>
      )}
      {anyLive && (
        <>
          <span className="sep">·</span>
          <span>{info?.isAdmin ? t("live.on") : t("live.off")}</span>
        </>
      )}
      <span className="grow" />
      {info && <span>v{info.version}</span>}
    </footer>
  );
}
