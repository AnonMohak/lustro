import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { requestAdmin } from "../lib/admin";
import { IS_WINDOWS } from "../lib/platform";
import { useStore } from "../store";
import { Icon } from "./Icon";
import { Logo } from "./Logo";

export function TitleBar() {
  const { t } = useTranslation();
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const win = getCurrentWindow();

  return (
    <header className="titlebar" data-tauri-drag-region>
      <Logo />
      <span className="appname" data-tauri-drag-region>
        {t("app.name")}
      </span>
      {isAdmin ? (
        <span className="chip admin">
          <Icon name="shield" />
          {t("titlebar.admin")}
        </span>
      ) : (
        IS_WINDOWS && (
          <button className="chip" onClick={requestAdmin}>
            <Icon name="shield" />
            {t("titlebar.runAsAdmin")}
          </button>
        )
      )}
      <span className="grow" data-tauri-drag-region />
      <div className="caps">
        <button aria-label={t("titlebar.minimize")} onClick={() => win.minimize()}>
          <Icon name="min" />
        </button>
        <button aria-label={t("titlebar.maximize")} onClick={() => win.toggleMaximize()}>
          <Icon name="max" />
        </button>
        <button className="x" aria-label={t("titlebar.close")} onClick={() => win.close()}>
          <Icon name="x" />
        </button>
      </div>
    </header>
  );
}
