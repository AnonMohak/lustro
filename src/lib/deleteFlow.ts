import i18n from "../i18n";
import { useStore } from "../store";
import { api, type DeleteTarget } from "./api";
import { formatBytes, formatCount } from "./format";

export interface DeleteItem extends DeleteTarget {
  name: string;
  path: string;
  size: number;
  /** A protected app folder in Program Files: it needs the extra warning. */
  appFolder?: boolean;
}

/** Asks for confirmation, deletes, then refreshes the views. */
export function confirmDelete(items: DeleteItem[], permanent: boolean, onDone?: () => void) {
  if (items.length === 0) return;
  const { openDialog, showToast, afterDelete, runJob } = useStore.getState();
  const t = i18n.t.bind(i18n);
  const bytes = items.reduce((a, i) => a + i.size, 0);
  const size = formatBytes(bytes);
  const one = items.length === 1 ? items[0] : null;
  const body = one ? [`${one.name} · ${size}`, one.path] : [t("del.manyBody", { count: items.length, n: formatCount(items.length), size })];
  body.push(permanent ? t("del.permanentWarn") : t("del.recycleNote"));
  // App folders in Program Files: a warning and an "I understand" checkbox, every time.
  const apps = items.filter((i) => i.appFolder).length;

  openDialog({
    title: apps > 0
      ? t("del.appTitle", { count: apps })
      : permanent
      ? one
        ? t("del.permanentTitleOne")
        : t("del.permanentTitleMany", { n: formatCount(items.length) })
      : one
        ? t("del.recycleTitleOne")
        : t("del.recycleTitleMany", { n: formatCount(items.length) }),
    body,
    confirm: permanent ? t("del.permanentConfirm") : t("del.recycleConfirm"),
    danger: permanent || apps > 0,
    warn: apps > 0 ? t("del.appWarn", { count: apps }) : undefined,
    ack: apps > 0 ? t("del.appAck") : undefined,
    // The sidebar block shows the progress and the result; a toast shows only problems.
    onConfirm: () =>
      runJob({
        kind: permanent ? "permanent" : "recycle",
        items: items.length,
        bytes,
        run: async () => {
          const r = await api.deleteItems(
            items.map(({ root, id }) => ({ root, id })),
            permanent,
            apps > 0,
          );
          afterDelete(r.summaries);
          if (r.failed.length > 0)
            showToast(
              t("del.partial", { ok: r.deleted, total: r.deleted + r.failed.length, reason: r.failed[0].message }),
              true,
            );
          onDone?.();
          return { freed: r.freedBytes, errors: r.failed.length };
        },
      }),
  });
}
