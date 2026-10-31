import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { Select } from "../components/Select";
import { api, type BackendSettings, type CleanRuleInfo, type CustomRule } from "../lib/api";
import { IS_WINDOWS } from "../lib/platform";
import { autoUpdateOn, checkForUpdates, setAutoUpdate } from "../lib/updates";

function Switch({ id, on, onChange }: { id: string; on: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="switch">
      <input type="checkbox" id={id} checked={on} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}
import { useStore } from "../store";

export function Settings() {
  const { t } = useTranslation();
  const version = useStore((s) => s.info?.version ?? "");
  const deleteMode = useStore((s) => s.deleteMode);
  const setDeleteMode = useStore((s) => s.setDeleteMode);
  const showToast = useStore((s) => s.showToast);
  const [backend, setBackend] = useState<BackendSettings | null>(null);
  /** Choices for the low space actions: the user's rules and the safe built-in rules. */
  const [myRules, setMyRules] = useState<CustomRule[]>([]);
  const [cleanRules, setCleanRules] = useState<CleanRuleInfo[]>([]);
  useEffect(() => {
    api
      .customRules()
      .then(setMyRules)
      .catch(() => {});
    api
      .cleanRulesList()
      .then((l) => setCleanRules(l.filter((r) => r.safe && !r.admin && !r.warn)))
      .catch(() => {});
  }, []);
  /** Adds or removes `id` in a list of the backend settings. */
  const toggleIn = (key: "lowSpaceRules" | "lowSpaceClean", id: string) =>
    backend &&
    update({ [key]: backend[key].includes(id) ? backend[key].filter((x) => x !== id) : [...backend[key], id] });
  useEffect(() => {
    api
      .getSettings()
      .then(setBackend)
      .catch((e) => showToast(String(e), true));
  }, [showToast]);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const setAllowAppFolders = useStore((s) => s.setAllowAppFolders);
  const update = (patch: Partial<BackendSettings>) => {
    if (!backend) return;
    const next = { ...backend, ...patch };
    setBackend(next);
    api
      .setSettings(next)
      .then(setBackend)
      .catch((e) => {
        showToast(String(e), true);
        api
          .getSettings()
          .then(setBackend)
          .catch(() => {});
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

      <h3 className="sec">{t("settings.delete")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.defaultDelete")}</div>
            <div className="sd">{t("settings.defaultDeleteHint")}</div>
          </div>
          <Select<"recycle" | "permanent">
            id="setDelete"
            ariaLabel={t("settings.defaultDelete")}
            value={deleteMode}
            options={[
              { value: "recycle", label: t("del.recycle") },
              { value: "permanent", label: t("del.permanent") },
            ]}
            onChange={setDeleteMode}
          />
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
        {backend && (
          <div className="srow">
            <div>
              <div>{t("settings.allowAppFolders")}</div>
              <div className="sd">{t("settings.allowAppFoldersHint")}</div>
            </div>
            {!isAdmin && (
              <span className="pill warn">
                <Icon name="shield" />
                {t("settings.needsAdmin")}
              </span>
            )}
            <Switch
              id="setAppFolders"
              on={backend.allowAppFolders}
              onChange={(v) => {
                update({ allowAppFolders: v });
                setAllowAppFolders(v);
              }}
            />
          </div>
        )}
      </div>

      <h3 className="sec">{t("settings.cleanup")}</h3>
      <div className="card set">
        <div className="srow">
          <div>
            <div>{t("settings.devAge")}</div>
            <div className="sd">{t("settings.devAgeHint")}</div>
          </div>
          <Select
            id="setDevAge"
            ariaLabel={t("settings.devAge")}
            value={devAgeDays}
            options={[7, 14, 30, 60, 90, 180].map((d) => ({ value: d, label: t("settings.days", { count: d }) }))}
            onChange={setDevAgeDays}
          />
        </div>
        {backend && (
          <>
            <div className="srow">
              <div>
                <div>{t("settings.ignore")}</div>
                <div className="sd">{t("settings.ignoreHint")}</div>
              </div>
              <button
                className="btn sm"
                onClick={async () => {
                  const dir = await open({ directory: true, title: t("settings.ignorePick") });
                  if (typeof dir === "string" && !backend.ignore.includes(dir))
                    update({ ignore: [...backend.ignore, dir] });
                }}
              >
                <Icon name="folder" />
                {t("settings.ignoreAdd")}
              </button>
            </div>
            {backend.ignore.map((f) => (
              <div key={f} className="srow">
                <div className="mono" style={{ overflowWrap: "anywhere" }}>
                  {f}
                </div>
                <button
                  className="btn ghost sm"
                  aria-label={t("settings.ignoreRemove", { path: f })}
                  onClick={() => update({ ignore: backend.ignore.filter((x) => x !== f) })}
                >
                  <Icon name="x" />
                </button>
              </div>
            ))}
          </>
        )}
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
            <Select
              id="setLowPct"
              ariaLabel={t("settings.lowSpace")}
              value={backend.lowSpacePercent}
              disabled={!backend.lowSpaceAlert}
              options={[5, 10, 15, 20].map((n) => ({ value: n, label: t("settings.percentFree", { n }) }))}
              onChange={(n) => update({ lowSpacePercent: n })}
            />
            <Switch id="setLow" on={backend.lowSpaceAlert} onChange={(v) => update({ lowSpaceAlert: v })} />
          </div>
          {backend.lowSpaceAlert && (myRules.length > 0 || cleanRules.length > 0) && (
            <div className="srow">
              <div>
                <div>{t("settings.lowSpaceActions")}</div>
                <div className="sd">{t("settings.lowSpaceActionsHint")}</div>
                <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 8 }}>
                  {myRules.map((r) => (
                    <label key={r.id} className="row" style={{ cursor: "pointer" }}>
                      <input
                        type="checkbox"
                        checked={backend.lowSpaceRules.includes(r.id)}
                        onChange={() => toggleIn("lowSpaceRules", r.id)}
                      />
                      <span>{r.name}</span>
                      <span className="faint">{t("settings.yourRule")}</span>
                    </label>
                  ))}
                  {cleanRules.map((r) => (
                    <label key={r.id} className="row" style={{ cursor: "pointer" }}>
                      <input
                        type="checkbox"
                        checked={backend.lowSpaceClean.includes(r.id)}
                        onChange={() => toggleIn("lowSpaceClean", r.id)}
                      />
                      <span>{r.name}</span>
                      <span className="faint">{t(`clean.groups.${r.group}`)}</span>
                    </label>
                  ))}
                </div>
              </div>
            </div>
          )}
          <div className="srow">
            <div>
              <div>{t("settings.startWithWindows")}</div>
              <div className="sd">{t("settings.startWithWindowsHint")}</div>
            </div>
            <Switch id="setStart" on={backend.startWithWindows} onChange={(v) => update({ startWithWindows: v })} />
          </div>
          {IS_WINDOWS && (
            <div className="srow">
              <div>
                <div>{t("settings.explorerMenu")}</div>
                <div className="sd">{t("settings.explorerMenuHint")}</div>
              </div>
              <Switch id="setMenu" on={backend.explorerMenu} onChange={(v) => update({ explorerMenu: v })} />
            </div>
          )}
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
