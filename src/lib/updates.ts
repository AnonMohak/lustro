import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import i18n from "../i18n";
import { useStore } from "../store";

const AUTO_KEY = "lustro.autoUpdate";

export function autoUpdateOn(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "off";
  } catch {
    return true;
  }
}

export function setAutoUpdate(on: boolean) {
  try {
    localStorage.setItem(AUTO_KEY, on ? "on" : "off");
  } catch {
    // Not saved; the choice still applies to this session.
  }
}

/**
 * Looks for a new version on GitHub Releases. `manual` shows the result even when there is
 * no update or the check fails; the automatic check at start only speaks up for an update.
 */
export async function checkForUpdates(manual: boolean) {
  const { showToast, openDialog } = useStore.getState();
  const t = i18n.t.bind(i18n);
  try {
    const update = await check();
    if (!update) {
      if (manual) showToast(t("update.upToDate"));
      return;
    }
    openDialog({
      title: t("update.available", { version: update.version }),
      body: [update.body?.trim() || t("update.noNotes"), t("update.restartNote")],
      confirm: t("update.install"),
      onConfirm: async () => {
        showToast(t("update.downloading"));
        try {
          await update.downloadAndInstall();
          await relaunch();
        } catch (e) {
          showToast(t("update.failed", { error: String(e) }), true);
        }
      },
    });
  } catch (e) {
    if (manual) showToast(t("update.failed", { error: String(e) }), true);
  }
}
