import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { api, type BackendSettings } from "../lib/api";
import { autoUpdateOn, checkForUpdates, setAutoUpdate } from "../lib/updates";

function Switch({ id, on, onChange }: { id: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="switch">
      <input type="checkbox" id={id} checked={on} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}
import { useStore, type Theme } from "../store";

const THEMES: Theme[] = ["system", "light", "dark"];
const THEME_KEYS = { system: "settings.themeSystem", light: "settings.themeLight", dark: "settings.themeDark" } as const;

export function Settings() {
  const { t } = useTranslation();
  const theme = useStore((s) => s.theme);
  const setTheme = useStore((s) => s.setTheme);
  const version = useStore((s) => s.info?.version ?? "");
  const deleteMode = useStore((s) => s.deleteMode);
  const setDeleteMode = useStore((s) => s.setDeleteMode);
  const showToast = useStore((s) => s.showToast);
  const [backend, setBackend] = useState<BackendSettings | null>(null);
  useEffect(() => {
    api.getSettings().then(setBackend).catch((e) => showToast(String(e), true));
  }, [showToast]);
  const update = (patch: Partial<BackendSettings>) => {
    if (!backend) return;
    const next = { ...backend, ...patch };
    setBackend(next);
    api
      .setSettings(next)
      .then(setBackend)
      .catch((e) => {
        showToast(String(e), true);
        api.getSettings().then(setBackend).catch(() => {});
      });
  };
  const [autoUpdate, setAuto] = useState(autoUpdateOn());
  const devAgeDays = useStore((s) => s.devAgeDays);
  const setDevAgeDays = useStore((s) => s.setDevAgeDays);

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

      <h3 className="sec">{t("settings.delete")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.defaultDelete")}</div>
            <div className="sd">{t("settings.defaultDeleteHint")}</div>
          </div>
          <select
            className="text"
            id="setDelete"
            value={deleteMode}
            onChange={(e) => setDeleteMode(e.target.value as "recycle" | "permanent")}
          >
            <option value="recycle">{t("del.recycle")}</option>
            <option value="permanent">{t("del.permanent")}</option>
          </select>
        </div>
        <div className="srow">
          <div>
            <div>{t("settings.askPermanent")}</div>
            <div className="sd">{t("settings.askPermanentHint")}</div>
          </div>
          <span className="pill good">
            <Icon name="check" />
            {t("settings.alwaysOn")}
          </span>
        </div>
        <div className="srow">
          <div>
            <div>{t("settings.protectedTitle")}</div>
            <div className="sd">{t("settings.protectedHint")}</div>
          </div>
          <span className="pill bad">
            <Icon name="lock" />
            {t("settings.alwaysOn")}
          </span>
        </div>
      </div>

      <h3 className="sec">{t("settings.cleanup")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.devAge")}</div>
            <div className="sd">{t("settings.devAgeHint")}</div>
          </div>
          <select className="text" id="setDevAge" value={devAgeDays} onChange={(e) => setDevAgeDays(Number(e.target.value))}>
            {[7, 14, 30, 60, 90, 180].map((d) => (
              <option key={d} value={d}>
                {t("settings.days", { count: d })}
              </option>
            ))}
          </select>
        </div>
      </div>

      <h3 className="sec">{t("settings.windows")}</h3>
      {backend && (
        <div className="card set">
          <div className="srow">
            <div>
              <div>{t("settings.keepInTray")}</div>
              <div className="sd">{t("settings.keepInTrayHint")}</div>
            </div>
            <Switch id="setTray" on={backend.keepInTray} onChange={(v) => update({ keepInTray: v })} />
          </div>
          <div className="srow">
            <div>
              <div>{t("settings.lowSpace")}</div>
              <div className="sd">{t("settings.lowSpaceHint")}</div>
            </div>
            <select
              className="text"
              id="setLowPct"
              value={backend.lowSpacePercent}
              disabled={!backend.lowSpaceAlert}
              onChange={(e) => update({ lowSpacePercent: Number(e.target.value) })}
            >
              {[5, 10, 15, 20].map((n) => (
                <option key={n} value={n}>
                  {t("settings.percentFree", { n })}
                </option>
              ))}
            </select>
            <Switch id="setLow" on={backend.lowSpaceAlert} onChange={(v) => update({ lowSpaceAlert: v })} />
          </div>
          <div className="srow">
            <div>
              <div>{t("settings.startWithWindows")}</div>
              <div className="sd">{t("settings.startWithWindowsHint")}</div>
            </div>
            <Switch id="setStart" on={backend.startWithWindows} onChange={(v) => update({ startWithWindows: v })} />
          </div>
          <div className="srow">
            <div>
              <div>{t("settings.explorerMenu")}</div>
              <div className="sd">{t("settings.explorerMenuHint")}</div>
            </div>
            <Switch id="setMenu" on={backend.explorerMenu} onChange={(v) => update({ explorerMenu: v })} />
          </div>
        </div>
      )}

      <h3 className="sec">{t("settings.about")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.version", { version })}</div>
            <div className="sd">{t("settings.aboutHint")}</div>
          </div>
          <button className="btn sm" onClick={() => checkForUpdates(true)}>
            <Icon name="refresh" />
            {t("nav.checkUpdates")}
          </button>
        </div>
        <div className="srow">
          <div>
            <div>{t("update.auto")}</div>
            <div className="sd">{t("update.autoHint")}</div>
          </div>
          <Switch
            id="setAutoUpdate"
            on={autoUpdate}
            onChange={(v) => {
              setAuto(v);
              setAutoUpdate(v);
            }}
          />
        </div>
      </div>
    </div>
  );
}
