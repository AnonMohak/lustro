import i18n from "../i18n";
import { useStore } from "../store";
import { api } from "./api";

/** Asks for confirmation, then restarts Lustro elevated. */
export function requestAdmin() {
  const { openDialog, showToast } = useStore.getState();
  const t = i18n.t.bind(i18n);
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
