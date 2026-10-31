import { downloadDir } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { Select } from "../components/Select";
import { api, type CleanItem, type CustomRule } from "../lib/api";
import { formatBytes, timeAgo } from "../lib/format";
import { PLATFORM } from "../lib/platform";
import { useStore } from "../store";

const SCHEDULES: CustomRule["schedule"][] = ["off", "start", "daily", "weekly"];

const NEW_RULE: CustomRule = {
  id: "",
  name: "",
  root: "",
  kind: "folders",
  targets: ["node_modules"],
  ageDays: 30,
  schedule: "off",
  lastRun: 0,
};

/** Installer and disk image types of each platform, for the "Old installers" preset. */
const INSTALLERS: Record<string, string[]> = {
  windows: ["*.exe", "*.msi", "*.msix", "*.iso"],
  macos: ["*.dmg", "*.pkg", "*.iso"],
  linux: ["*.deb", "*.rpm", "*.AppImage", "*.iso"],
  android: ["*.apk", "*.apks", "*.xapk"],
};

/** The form for a new or changed rule. `targets` is edited as text: "node_modules, target". */
function RuleForm({
  rule,
  onSave,
  onCancel,
}: {
  rule: CustomRule;
  onSave: (r: CustomRule) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [r, setR] = useState(rule);
  const [targets, setTargets] = useState(rule.targets.join(", "));

  const pick = async () => {
    const dir = await open({ directory: true, title: t("custom.pickFolder") });
    if (typeof dir === "string")
      setR((x) => ({ ...x, root: dir, name: x.name || t("custom.defaultName", { names: targets }) }));
  };

  return (
    <div className="card set">
      <div className="srow">
        <div>
          <div>{t("custom.folder")}</div>
          <div className="sd">{t("custom.folderHint")}</div>
        </div>
        <input
          className="text mono"
          readOnly
          value={r.root}
          placeholder={t("custom.noFolder")}
          style={{ width: 280 }}
          aria-label={t("custom.folder")}
        />
        <button className="btn sm" onClick={pick}>
          <Icon name="folder" />
          {t("custom.choose")}
        </button>
      </div>
      <div className="srow">
        <div>
          <div>{t("custom.match")}</div>
        </div>
        <div className="seg">
          <button className={r.kind === "folders" ? "on" : ""} onClick={() => setR({ ...r, kind: "folders" })}>
            {t("custom.matchFolders")}
          </button>
          <button className={r.kind === "files" ? "on" : ""} onClick={() => setR({ ...r, kind: "files" })}>
            {t("custom.matchFiles")}
          </button>
        </div>
      </div>
      <div className="srow">
        <div>
          <div>{r.kind === "files" ? t("custom.patterns") : t("custom.targets")}</div>
          <div className="sd">{r.kind === "files" ? t("custom.patternsHint") : t("custom.targetsHint")}</div>
        </div>
        <input
          className="text mono"
          value={targets}
          onChange={(e) => setTargets(e.target.value)}
          style={{ width: 280 }}
          aria-label={t("custom.targets")}
        />
      </div>
      <div className="srow">
        <div>
          <div>{t("custom.age")}</div>
          <div className="sd">{r.kind === "files" ? t("custom.ageHintFiles") : t("custom.ageHint")}</div>
        </div>
        <input
          className="text"
          type="number"
          min={1}
          value={r.ageDays}
          onChange={(e) => setR({ ...r, ageDays: Math.max(0, Math.floor(Number(e.target.value) || 0)) })}
          style={{ width: 80 }}
          aria-label={t("custom.age")}
        />
        <span className="faint">{t("custom.days")}</span>
      </div>
      <div className="srow">
        <div>
          <div>{t("custom.schedule")}</div>
          <div className="sd">{t("custom.scheduleHint")}</div>
        </div>
        <Select
          ariaLabel={t("custom.schedule")}
          value={r.schedule}
          options={SCHEDULES.map((s) => ({ value: s, label: t(`custom.schedules.${s}`) }))}
          onChange={(schedule) => setR({ ...r, schedule })}
        />
      </div>
      <div className="srow">
        <div>
          <div>{t("custom.name")}</div>
        </div>
        <input
          className="text"
          value={r.name}
          onChange={(e) => setR({ ...r, name: e.target.value })}
          style={{ width: 280 }}
          aria-label={t("custom.name")}
        />
      </div>
      <div className="srow">
        <div />
        <button className="btn" onClick={onCancel}>
          {t("common.cancel")}
        </button>
        <button
          className="btn accent"
          onClick={() =>
            onSave({
              ...r,
              targets: targets
                .split(",")
                .map((x) => x.trim())
                .filter(Boolean),
            })
          }
        >
          {t("custom.save")}
        </button>
      </div>
    </div>
  );
}

/** The user's own clean rules on the Clean screen: list, edit, preview, run. */
export function CustomRules() {
  const { t } = useTranslation();
  const showToast = useStore((s) => s.showToast);
  const openDialog = useStore((s) => s.openDialog);
  const [rules, setRules] = useState<CustomRule[] | null>(null);
  /** The rule in the form: a copy of a rule, or NEW_RULE. */
  const [editing, setEditing] = useState<CustomRule | null>(null);
  const [previews, setPreviews] = useState<Record<string, CleanItem[] | "loading">>({});
  const [running, setRunning] = useState<string | null>(null);

  const load = useCallback(() => {
    api
      .customRules()
      .then(setRules)
      .catch((e) => showToast(String(e), true));
  }, [showToast]);
  useEffect(load, [load]);

  const save = async (list: CustomRule[]) => {
    try {
      setRules(await api.setCustomRules(list));
      return true;
    } catch (e) {
      showToast(String(e), true);
      return false;
    }
  };

  const saveForm = async (r: CustomRule) => {
    if (!rules) return;
    const list = r.id ? rules.map((x) => (x.id === r.id ? r : x)) : [...rules, r];
    if (await save(list)) {
      setEditing(null);
      hidePreview(r.id);
    }
  };

  const hidePreview = (id: string) =>
    setPreviews((p) => {
      const n = { ...p };
      delete n[id];
      return n;
    });

  /** Shows the matches, or hides them when they show. */
  const preview = async (r: CustomRule) => {
    const shown = previews[r.id];
    if (Array.isArray(shown) && shown.length > 0) {
      hidePreview(r.id);
      return;
    }
    setPreviews((p) => ({ ...p, [r.id]: "loading" }));
    try {
      const items = await api.customRuleScan(r.id);
      setPreviews((p) => ({ ...p, [r.id]: items }));
      if (items.length === 0) showToast(t("custom.noMatches"));
    } catch (e) {
      setPreviews((p) => ({ ...p, [r.id]: [] }));
      showToast(String(e), true);
    }
  };

  const run = (r: CustomRule) =>
    openDialog({
      title: t("custom.runTitle", { name: r.name }),
      body: [t("custom.runBody", { count: r.ageDays, names: r.targets.join(", ") })],
      confirm: t("custom.runNow"),
      onConfirm: async () => {
        setRunning(r.id);
        try {
          const out = await api.customRuleRun(r.id);
          if (out.folders === 0 && out.errors === 0) showToast(t("custom.noneDue"));
          else if (out.errors > 0)
            showToast(
              t("custom.doneErrors", { count: out.folders, size: formatBytes(out.freedBytes), errors: out.errors }),
              true,
            );
          else showToast(t("custom.done", { count: out.folders, size: formatBytes(out.freedBytes) }));
        } catch (e) {
          showToast(String(e), true);
        }
        setRunning(null);
        hidePreview(r.id);
        load();
      },
    });

  const remove = (r: CustomRule) =>
    openDialog({
      title: t("custom.deleteTitle", { name: r.name }),
      body: [t("custom.deleteBody")],
      confirm: t("custom.delete"),
      danger: true,
      onConfirm: () => rules && save(rules.filter((x) => x.id !== r.id)),
    });

  if (!rules) return null;
  return (
    <div className="page">
      <div className="phead">
        <div>
          <h2>{t("custom.title")}</h2>
          <p>{t("custom.hint")}</p>
        </div>
        <div className="row">
          <button
            className="btn sm"
            disabled={editing !== null}
            onClick={async () =>
              setEditing({
                ...NEW_RULE,
                name: t("custom.installersName"),
                root: await downloadDir().catch(() => ""),
                kind: "files",
                targets: INSTALLERS[PLATFORM] ?? INSTALLERS.windows,
                ageDays: 14,
              })
            }
          >
            {t("custom.presetInstallers")}
          </button>
          <button className="btn sm accent" disabled={editing !== null} onClick={() => setEditing(NEW_RULE)}>
            {t("custom.add")}
          </button>
        </div>
      </div>
      {editing && !editing.id && <RuleForm rule={editing} onSave={saveForm} onCancel={() => setEditing(null)} />}
      {rules.length === 0 && !editing && (
        <div className="empty">
          <span className="ei">
            <Icon name="clean" />
          </span>
          <h3>{t("custom.emptyTitle")}</h3>
          <p>{t("custom.emptyBody")}</p>
        </div>
      )}
      {rules.length > 0 && (
        <div className="card">
          {rules.map((r) => {
            if (editing?.id === r.id)
              return <RuleForm key={r.id} rule={editing} onSave={saveForm} onCancel={() => setEditing(null)} />;
            const items = previews[r.id];
            const due = Array.isArray(items) ? items.filter((i) => i.selected) : [];
            return (
              <div key={r.id} className="citem">
                <Icon name="clean" />
                <div className="cn">
                  {r.name}
                  <span className="pill acc">{t(`custom.schedules.${r.schedule}`)}</span>
                  <span className="pill">
                    {r.kind === "files"
                      ? t("custom.olderThanFiles", { count: r.ageDays })
                      : t("custom.olderThan", { count: r.ageDays })}
                  </span>
                </div>
                <div className="row">
                  <button className="btn sm ghost" onClick={() => preview(r)} disabled={items === "loading"}>
                    {items === "loading" ? t("custom.looking") : t("custom.preview")}
                  </button>
                  <button className="btn sm" onClick={() => run(r)} disabled={running !== null}>
                    {running === r.id ? t("custom.running") : t("custom.runNow")}
                  </button>
                  <button className="btn sm ghost" onClick={() => setEditing({ ...r })} disabled={editing !== null}>
                    {t("custom.edit")}
                  </button>
                  <button className="btn sm ghost" aria-label={t("custom.delete")} onClick={() => remove(r)}>
                    <Icon name="trash" />
                  </button>
                </div>
                <div className="cd">
                  <span className="mono" title={r.root}>
                    {r.root}
                  </span>{" "}
                  {r.targets.map((x) => (
                    <span key={x} className="cmd">
                      {x}
                    </span>
                  ))}{" "}
                  <span className="faint">
                    {r.lastRun ? t("custom.lastRun", { ago: timeAgo(r.lastRun) }) : t("custom.neverRun")}
                  </span>
                </div>
                {Array.isArray(items) && items.length > 0 && (
                  <div className="subs">
                    <div className="sub" style={{ cursor: "default" }}>
                      <span />
                      <b>
                        {t("custom.dueNow", {
                          count: due.length,
                          size: formatBytes(due.reduce((a, i) => a + i.bytes, 0)),
                        })}
                      </b>
                      <span />
                    </div>
                    {items.map((i) => (
                      <div key={i.path} className="sub" style={{ cursor: "default", opacity: i.selected ? 1 : 0.55 }}>
                        <input type="checkbox" checked={i.selected} disabled aria-label={i.path} />
                        <span className="sub-name">
                          <span className="mono" title={i.path}>
                            {i.path}
                          </span>
                          <span className="faint">
                            {i.ageDays === null
                              ? t("custom.ageUnknown")
                              : r.kind === "files"
                                ? t("custom.fileAge", { count: i.ageDays })
                                : t("custom.untouched", { count: i.ageDays })}
                          </span>
                        </span>
                        <span className="num">{formatBytes(i.bytes)}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
