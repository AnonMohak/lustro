import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "../store";
import { Icon } from "./Icon";

export function Dialog() {
  const { t } = useTranslation();
  const dialog = useStore((s) => s.dialog);
  const close = useStore((s) => s.closeDialog);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!dialog) return;
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dialog, close]);

  if (!dialog) return null;
  return (
    <div className="overlay" onClick={(e) => e.target === e.currentTarget && close()}>
      <div className="dlg" role="dialog" aria-modal="true" aria-labelledby="dlg-title">
        <div className="db">
          <h3 id="dlg-title">{dialog.title}</h3>
          {dialog.body.map((p, i) => (
            <p key={i}>{p}</p>
          ))}
        </div>
        <div className="df">
          <button
            className={`btn ${dialog.danger ? "danger-fill" : "accent"}`}
            onClick={() => {
              close();
              dialog.onConfirm();
            }}
          >
            {dialog.confirm}
          </button>
          <button className="btn" ref={cancelRef} onClick={close}>
            {t("common.cancel")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function Toast() {
  const toast = useStore((s) => s.toast);
  if (!toast) return null;
  return (
    <div className={`toast ${toast.error ? "error" : ""}`} role="status">
      <Icon name={toast.error ? "warn" : "check"} />
      <span>{toast.message}</span>
    </div>
  );
}
