import { useTranslation } from "react-i18next";
import { formatBytes } from "../lib/format";
import { useStore } from "../store";

export function StatusBar() {
  const { t } = useTranslation();
  const info = useStore((s) => s.info);
  const drives = useStore((s) => s.drives);
  const free = drives?.reduce((a, d) => a + d.freeBytes, 0) ?? 0;

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
      <span className="grow" />
      {info && <span>v{info.version}</span>}
    </footer>
  );
}
