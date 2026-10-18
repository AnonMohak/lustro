import { useTranslation } from "react-i18next";
import { useStore, type Theme } from "../store";

const THEMES: Theme[] = ["system", "light", "dark"];
const THEME_KEYS = { system: "settings.themeSystem", light: "settings.themeLight", dark: "settings.themeDark" } as const;

export function Settings() {
  const { t } = useTranslation();
  const theme = useStore((s) => s.theme);
  const setTheme = useStore((s) => s.setTheme);
  const version = useStore((s) => s.info?.version ?? "");

  return (
    <div className="page" style={{ maxWidth: 820 }}>
      <div className="phead">
        <h2>{t("settings.title")}</h2>
      </div>

      <h3 className="sec">{t("settings.appearance")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.theme")}</div>
            <div className="sd">{t("settings.themeHint")}</div>
          </div>
          <div className="seg" role="group" aria-label={t("settings.theme")}>
            {THEMES.map((v) => (
              <button key={v} className={theme === v ? "on" : ""} onClick={() => setTheme(v)}>
                {t(THEME_KEYS[v])}
              </button>
            ))}
          </div>
        </div>
      </div>

      <h3 className="sec">{t("settings.about")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.version", { version })}</div>
            <div className="sd">{t("settings.aboutHint")}</div>
          </div>
        </div>
      </div>
    </div>
  );
}
