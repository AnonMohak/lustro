import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../components/Icon";

/** Shown for screens that a later milestone builds. */
export function ComingSoon({ screen, milestone, icon }: { screen: string; milestone: string; icon: IconName }) {
  const { t } = useTranslation();
  return (
    <div className="empty">
      <span className="ei">
        <Icon name={icon} />
      </span>
      <h3>{t("soon.title", { screen })}</h3>
      <p>{t("soon.body", { milestone })}</p>
    </div>
  );
}
