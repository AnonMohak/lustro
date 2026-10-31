import i18n from "../i18n";
import { useStore } from "../store";
import { api } from "./api";
import { IS_WINDOWS } from "./platform";

/** Asks for confirmation, then restarts Lustro elevated. Other platforms cannot restart
 *  Lustro as root, so they only say how to start it as root. */
export function requestAdmin() {
  const { openDialog, showToast } = useStore.getState();
  const t = i18n.t.bind(i18n);
  if (!IS_WINDOWS) {
    showToast(t("admin.unixHint"));
    return;
  }
  openDialog({
    title: t("admin.dialogTitle"),
    body: [t("admin.dialogBody1"), t("admin.dialogBody2")],
    confirm: t("admin.confirm"),
    onConfirm: async () => {
      try {
        await api.restartAsAdmin();
      } catch (e) {
        const msg = String(e);
        if (msg.includes("cancelled")) showToast(t("admin.cancelled"), true);
        else showToast(t("admin.failed", { error: msg }), true);
      }
    },
  });
}
