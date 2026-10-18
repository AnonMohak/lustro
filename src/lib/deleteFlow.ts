import i18n from "../i18n";
import { useStore } from "../store";
import { api, type DeleteTarget } from "./api";
import { formatBytes, formatCount } from "./format";

export interface DeleteItem extends DeleteTarget {
  name: string;
  path: string;
  size: number;
}

/** Asks for confirmation, deletes, then refreshes the views. */
export function confirmDelete(items: DeleteItem[], permanent: boolean, onDone?: () => void) {
  if (items.length === 0) return;
  const { openDialog, showToast, afterDelete } = useStore.getState();
  const t = i18n.t.bind(i18n);
  const size = formatBytes(items.reduce((a, i) => a + i.size, 0));
  const one = items.length === 1 ? items[0] : null;
  const body = one ? [`${one.name} · ${size}`, one.path] : [t("del.manyBody", { count: items.length, n: formatCount(items.length), size })];
  body.push(permanent ? t("del.permanentWarn") : t("del.recycleNote"));

  openDialog({
    title: permanent
      ? one
        ? t("del.permanentTitleOne")
        : t("del.permanentTitleMany", { n: formatCount(items.length) })
      : one
        ? t("del.recycleTitleOne")
        : t("del.recycleTitleMany", { n: formatCount(items.length) }),
    body,
    confirm: permanent ? t("del.permanentConfirm") : t("del.recycleConfirm"),
    danger: permanent,
    onConfirm: async () => {
      try {
        const r = await api.deleteItems(
          items.map(({ root, id }) => ({ root, id })),
          permanent,
        );
        afterDelete(r.summaries);
        if (r.failed.length === 0) {
          showToast(
            t(permanent ? "del.doneDeleted" : "del.doneRecycled", {
              count: r.deleted,
              n: formatCount(r.deleted),
              size: formatBytes(r.freedBytes),
            }),
          );
        } else {
          showToast(
            t("del.partial", { ok: r.deleted, total: r.deleted + r.failed.length, reason: r.failed[0].message }),
            true,
          );
        }
        onDone?.();
      } catch (e) {
        showToast(String(e), true);
      }
    },
  });
}
