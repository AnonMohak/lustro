import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useStore } from "../store";
import { Icon } from "./Icon";

export function Dialog() {
  const { t } = useTranslation();
  const dialog = useStore((s) => s.dialog);
  const close = useStore((s) => s.closeDialog);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [acked, setAcked] = useState(false);

  useEffect(() => {
    if (!dialog) return;
    setAcked(false);
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
          {dialog.warn && (
            <div className="note lock">
              <Icon name="warn" />
              <span>{dialog.warn}</span>
            </div>
          )}
          {dialog.ack && (
            <label className="ack">
              <input type="checkbox" id="dlgAck" checked={acked} onChange={(e) => setAcked(e.target.checked)} />
              <span>{dialog.ack}</span>
            </label>
          )}
        </div>
        <div className="df">
          <button
            className={`btn ${dialog.danger ? "danger-fill" : "accent"}`}
            disabled={!!dialog.ack && !acked}
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
