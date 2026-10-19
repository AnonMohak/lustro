import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../components/Icon";
import { Treemap } from "../components/Treemap";
import { api, ROOT_ID, type NodeDetail, type NodeDto, type TreemapNode } from "../lib/api";
import { requestAdmin } from "../lib/admin";
import { confirmDelete } from "../lib/deleteFlow";
import { saveExport, scanReportHtml } from "../lib/exporters";
import { driveIcon, driveName } from "../lib/drives";
import { formatBytes, formatCount, percent } from "../lib/format";
import { KIND_ICON, kindColor, sortedBreakdown } from "../lib/kinds";
import { rootLabel, scanSummaryText } from "../lib/scanText";
import { useAppsUnlocked, useStore } from "../store";

const CHILD_LIMIT = 1000;
const ROW_H = 30;

/** Any folder can be a favourite, except a drive root. */
function canStar(root: string, n: NodeDto): boolean {
  return n.isDir && !(n.id === ROOT_ID && /^[A-Z]:\\$/.test(root));
}

function ScanProgressView({ root }: { root: string }) {
  const { t } = useTranslation();
  const p = useStore((s) => s.scanning?.progress);
  return (
    <div className="empty" style={{ maxWidth: 560 }}>
      <span className="ei">
        <Icon name="refresh" className="spin" />
      </span>
      <h3>{t("scan.scanning", { root: rootLabel(root) })}</h3>
      {p ? (
        <>
          <div className="big">{formatCount(p.files)}</div>
          <p>
            {t("scan.filesFound")} · {t("scan.foldersFound", { count: p.dirs, n: formatCount(p.dirs) })} ·{" "}
            {p.fast ? t("scan.mftRead", { size: formatBytes(p.bytes) }) : t("scan.bytesFound", { size: formatBytes(p.bytes) })}
          </p>
          <p className="mono faint scan-current" title={p.current}>
            {p.fast ? t("scan.method.mft") : p.current || " "}
          </p>
        </>
      ) : (
        <p>{t("scan.starting")}</p>
      )}
      <button className="btn" onClick={() => api.cancelScan()}>
        {t("scan.cancel")}
      </button>
    </div>
  );
}

interface Row {
  node: NodeDto;
  depth: number;
  parentSize: number;
  more?: number;
}

function TreePane({
  root,
  rootNode,
  sel,
  expanded,
  kids,
  favIds,
  onSelect,
  onToggle,
  onStar,
}: {
  root: string;
  rootNode: NodeDto | null;
  sel: number;
  expanded: Set<number>;
  kids: Map<number, NodeDto[]>;
  favIds: Set<number>;
  onSelect: (n: NodeDto) => void;
  onToggle: (n: NodeDto) => void;
  onStar: (id: number) => void;
}) {
  const { t } = useTranslation();
  const [filter, setFilter] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({ top: 0, h: 600 });

  const rows = useMemo(() => {
    const out: Row[] = [];
    const f = filter.trim().toLowerCase();
    const add = (parentId: number, parentSize: number, depth: number) => {
      const list = kids.get(parentId) ?? [];
      for (const n of list) {
        if (!f || n.name.toLowerCase().includes(f)) out.push({ node: n, depth, parentSize });
        if (expanded.has(n.id)) add(n.id, n.size, depth + 1);
      }
      if (list.length === CHILD_LIMIT) out.push({ node: list[0], depth, parentSize, more: CHILD_LIMIT });
    };
    add(ROOT_ID, rootNode?.size ?? 0, 0);
    return out;
  }, [kids, expanded, rootNode, filter]);

  // Keep the selected row visible.
  useEffect(() => {
    const el = scroller.current;
    const i = rows.findIndex((r) => r.node.id === sel && !r.more);
    if (!el || i < 0) return;
    const y = i * ROW_H;
    if (y < el.scrollTop) el.scrollTop = y;
    else if (y + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = y + ROW_H - el.clientHeight;
  }, [sel, rows]);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScroll((s) => ({ ...s, h: el.clientHeight })));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const first = Math.max(0, Math.floor(scroll.top / ROW_H) - 10);
  const last = Math.min(rows.length, Math.ceil((scroll.top + scroll.h) / ROW_H) + 10);

  return (
    <section className="list">
      <div className="lhead">
        <div className="ttl">
          <h3>{rootLabel(root)}</h3>
          <span className="faint num">{rootNode ? formatBytes(rootNode.size) : ""}</span>
        </div>
        <input
          className="text"
          id="treeFilter"
          placeholder={t("explore.filter")}
          aria-label={t("explore.filter")}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>
      <div className="lscroll" ref={scroller} onScroll={(e) => setScroll({ top: e.currentTarget.scrollTop, h: e.currentTarget.clientHeight })}>
        <div style={{ height: rows.length * ROW_H, position: "relative" }}>
          {rows.slice(first, last).map((r, k) => {
            const i = first + k;
            const n = r.node;
            const style = { top: i * ROW_H, ["--d" as string]: r.depth, ["--c" as string]: kindColor(n.kind) };
            if (r.more)
              return (
                <div key={`more-${n.id}-${i}`} className="trow more" style={style}>
                  <span />
                  <span />
                  <span className="tname faint">{t("explore.moreItems", { count: r.more })}</span>
                </div>
              );
            const open = expanded.has(n.id);
            return (
              <div
                key={n.id}
                className={`trow ${n.id === sel ? "sel" : ""}`}
                style={style}
                title={n.name}
                onClick={() => onSelect(n)}
              >
                <button
                  className={`tw ${open ? "open" : ""}`}
                  aria-label="Expand"
                  style={n.hasChildren ? undefined : { visibility: "hidden" }}
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggle(n);
                  }}
                >
                  <Icon name="chev" />
                </button>
                <span className="ticon" style={{ color: "var(--c)" }}>
                  <Icon name={n.isDir ? "folder" : KIND_ICON[n.kind]} />
                </span>
                <span className="tname">
                  {n.name}
                  {n.link && <Icon name="open" />}
                </span>
                {n.isDir ? (
                  <button
                    className={`star ${favIds.has(n.id) ? "on" : ""}`}
                    title={t(favIds.has(n.id) ? "explore.unstar" : "explore.star")}
                    aria-label={t(favIds.has(n.id) ? "explore.unstar" : "explore.star")}
                    onClick={(e) => {
                      e.stopPropagation();
                      onStar(n.id);
                    }}
                  >
                    <Icon name="star" className={favIds.has(n.id) ? "fill" : undefined} />
                  </button>
                ) : (
                  <span />
                )}
                <span className={`tsize ${n.denied ? "deny" : ""}`}>{n.denied ? t("scan.needsAdmin") : formatBytes(n.size)}</span>
                <span className="tbar">
                  <i style={{ width: `${percent(n.size, r.parentSize)}%` }} />
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}

function TypesView({ detail }: { detail: NodeDetail }) {
  const { t } = useTranslation();
  const ent = sortedBreakdown(detail.breakdown);
  const max = ent[0]?.[1] ?? 1;
  const total = detail.node.size || 1;
  return (
    <div className="types">
      <div className="muted">
        {t("explore.typesIn")} <span className="mono">{detail.path}</span>
      </div>
      {ent.map(([k, v]) => (
        <div key={k} className="trow2" style={{ ["--c" as string]: kindColor(k) }}>
          <span className="ftype">
            <Icon name={KIND_ICON[k]} />
          </span>
          <span>{t(`kind.${k}`)}</span>
          <span className="hbar">
            <i style={{ width: `${(v / max) * 100}%` }} />
          </span>
          <span className="num" style={{ textAlign: "right" }}>
            {formatBytes(v)}
          </span>
          <span className="num faint" style={{ textAlign: "right" }}>
            {((v / total) * 100).toFixed(1)}%
          </span>
        </div>
      ))}
    </div>
  );
}

function DetailPane({
  root,
  detail,
  rootSize,
  starred,
  onDelete,
  onStar,
}: {
  root: string;
  detail: NodeDetail | null;
  rootSize: number;
  starred: boolean;
  onDelete: (permanent: boolean) => void;
  onStar: (id: number) => void;
}) {
  const { t } = useTranslation();
  const showToast = useStore((s) => s.showToast);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const appsUnlocked = useAppsUnlocked();
  if (!detail) return <aside className="detail"><div className="dt muted">{t("explore.selectHint")}</div></aside>;
  const n = detail.node;
  const ent = sortedBreakdown(detail.breakdown);
  const share = percent(n.size, rootSize);
  return (
    <aside className="detail">
      <div className="dt" style={{ ["--c" as string]: kindColor(n.kind) }}>
        <div className="dt-hero">
          <span className="dt-ic">
            <Icon name={n.isDir ? "folder" : KIND_ICON[n.kind]} />
          </span>
          <div className="dt-name selectable">{n.id === ROOT_ID ? rootLabel(root) : n.name}</div>
          <div className="dt-path mono selectable">{detail.path}</div>
        </div>
        {n.denied && !isAdmin && (
          <div className="note warn">
            <Icon name="shield" />
            <span>
              {t("explore.denied")}{" "}
              <button className="linkbtn" onClick={requestAdmin}>
                {t("titlebar.runAsAdmin")}
              </button>
            </span>
          </div>
        )}
        {n.link && <div className="note"><Icon name="info" /><span>{t("explore.link")}</span></div>}
        {n.onlineOnly && <div className="note"><Icon name="info" /><span>{t("explore.onlineOnly")}</span></div>}
        <div>
          <div className="big">{formatBytes(n.size)}</div>
          <div className="faint">
            {n.isDir ? t("explore.folder") : t("explore.file")} · {t(`kind.${n.kind}`)}
          </div>
        </div>
        <dl className="kv">
          <dt>{t("explore.onDisk")}</dt>
          <dd>{formatBytes(n.alloc)}</dd>
          {n.isDir && (
            <>
              <dt>{t("explore.items")}</dt>
              <dd>{formatCount(n.items)}</dd>
            </>
          )}
          {n.modified > 0 && (
            <>
              <dt>{t("explore.modified")}</dt>
              <dd>{new Date(n.modified).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })}</dd>
            </>
          )}
          <dt>{t("explore.share", { root: rootLabel(root) })}</dt>
          <dd>{share.toFixed(1)}%</dd>
        </dl>
        <div className="share">
          <div className="bar">
            <i style={{ width: `${Math.max(0.5, share)}%` }} />
          </div>
        </div>
        {n.isDir && ent.length > 0 && (
          <div className="share">
            <h4>{t("explore.inside")}</h4>
            <div className="stackbar">
              {ent.map(([k, v]) => (
                <i key={k} style={{ ["--c" as string]: kindColor(k), width: `${percent(v, n.size)}%` }} />
              ))}
            </div>
            <div className="minilegend">
              {ent.slice(0, 5).map(([k, v]) => (
                <div key={k} style={{ ["--c" as string]: kindColor(k) }}>
                  <i />
                  <span>{t(`kind.${k}`)}</span>
                  <span>{formatBytes(v)}</span>
                </div>
              ))}
            </div>
          </div>
        )}
        <div className="acts">
          <div className="two">
            <button className="btn sm" onClick={() => api.revealPath(detail.path).catch((e) => showToast(String(e), true))}>
              <Icon name="open" />
              {t("explore.reveal")}
            </button>
            <button
              className="btn sm"
              onClick={() =>
                navigator.clipboard.writeText(detail.path).then(
                  () => showToast(t("explore.copied")),
                  (e) => showToast(String(e), true),
                )
              }
            >
              <Icon name="copy" />
              {t("explore.copyPath")}
            </button>
          </div>
          {canStar(root, n) && (
            <button className="btn sm block" onClick={() => onStar(n.id)}>
              <Icon name="star" className={starred ? "fill" : undefined} />
              {t(starred ? "explore.unstar" : "explore.star")}
            </button>
          )}
          {detail.protected && !(detail.appFolder && appsUnlocked) ? (
            <div className="note lock">
              <Icon name="lock" />
              <span>{t(detail.appFolder ? "explore.appFolderLocked" : "explore.protected")}</span>
            </div>
          ) : n.id === ROOT_ID ? (
            <div className="note">
              <Icon name="info" />
              <span>{t("explore.scanRoot")}</span>
            </div>
          ) : (
            <>
              {detail.appFolder && (
                <div className="note warn">
                  <Icon name="warn" />
                  <span>{t("explore.appFolder")}</span>
                </div>
              )}
              <button className="btn accent block" onClick={() => onDelete(false)}>
                <Icon name="trash" />
                {t("del.recycle")}
              </button>
              <button className="btn ghost danger block sm" onClick={() => onDelete(true)}>
                {t("del.permanent")}
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
  );
}

function ExploreView({ root }: { root: string }) {
  const { t } = useTranslation();
  const summary = useStore((s) => s.scans[root]);
  const isAdmin = useStore((s) => s.info?.isAdmin ?? false);
  const startScan = useStore((s) => s.startScan);
  const showToast = useStore((s) => s.showToast);

  const [zoom, setZoom] = useState(ROOT_ID);
  const [sel, setSel] = useState(ROOT_ID);
  const [view, setView] = useState<"map" | "types">("map");
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const [kids, setKids] = useState<Map<number, NodeDto[]>>(new Map());
  const [rootNode, setRootNode] = useState<NodeDto | null>(null);
  const [zoomDetail, setZoomDetail] = useState<NodeDetail | null>(null);
  const [selDetail, setSelDetail] = useState<NodeDetail | null>(null);
  const [map, setMap] = useState<TreemapNode | null>(null);
  const [protectedIds, setProtectedIds] = useState<Set<number>>(new Set());
  const treeVersion = useStore((s) => s.treeVersion);
  const scanVersion = summary?.scannedAt;
  const version = `${scanVersion}-${treeVersion}`;

  const loadKids = useCallback(
    async (id: number) => {
      if (kids.has(id)) return;
      const list = await api.listChildren(root, id, CHILD_LIMIT);
      setKids((m) => new Map(m).set(id, list));
    },
    [root, kids],
  );

  // A new scan resets the view.
  useEffect(() => {
    setZoom(ROOT_ID);
    setSel(ROOT_ID);
    setExpanded(new Set());
    setKids(new Map());
    setProtectedIds(new Set());
    api.nodeDetail(root, ROOT_ID).then((d) => setRootNode(d.node)).catch(() => {});
    // Merge, so that a favourite opened at the same time keeps its children.
    api.listChildren(root, ROOT_ID, CHILD_LIMIT).then((l) => setKids((m) => new Map(m).set(ROOT_ID, l))).catch(() => {});
  }, [root, scanVersion]);

  // A favourite from the sidebar: zoom into it and select it.
  const target = useStore((s) => s.exploreTarget);
  useEffect(() => {
    if (!target || target.root !== root) return;
    useStore.setState({ exploreTarget: null });
    setZoom(target.id);
    setSel(target.id);
    setExpanded((s) => new Set(s).add(target.id));
    api.listChildren(root, target.id, CHILD_LIMIT).then((l) => setKids((m) => new Map(m).set(target.id, l))).catch(() => {});
  }, [target, root, scanVersion]);

  const favorites = useStore((s) => s.favorites);
  const setFavorite = useStore((s) => s.setFavorite);
  const favIds = useMemo(
    () => new Set(favorites.flatMap((f) => f.hits.filter((h) => h.root === root).map((h) => h.id))),
    [favorites, root],
  );
  const toggleStar = useCallback(
    (id: number) => {
      const on = !favIds.has(id);
      api
        .nodeDetail(root, id)
        .then((d) => setFavorite(d.path, on))
        .catch((e) => showToast(String(e), true));
    },
    [root, favIds, setFavorite, showToast],
  );

  // After a delete: load the open folders again, but keep the view.
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;
  useEffect(() => {
    if (treeVersion === 0) return;
    const ids = [ROOT_ID, ...expandedRef.current];
    api.nodeDetail(root, ROOT_ID).then((d) => setRootNode(d.node)).catch(() => {});
    Promise.all(ids.map((id) => api.listChildren(root, id, CHILD_LIMIT).then((l) => [id, l] as const).catch(() => null)))
      .then((pairs) => setKids(new Map(pairs.filter((p) => p !== null))))
      .catch(() => {});
  }, [root, treeVersion]);

  useEffect(() => {
    let live = true;
    Promise.all([api.nodeDetail(root, zoom), api.treemap(root, zoom, 5000)])
      .then(([d, m]) => {
        if (!live) return;
        setZoomDetail(d);
        setMap(m);
      })
      .catch((e) => showToast(String(e), true));
    return () => {
      live = false;
    };
  }, [root, zoom, version, showToast]);

  useEffect(() => {
    let live = true;
    api
      .nodeDetail(root, sel)
      .then(async (d) => {
        if (!live) return;
        setSelDetail(d);
        if (d.protected) setProtectedIds((s) => (s.has(d.node.id) ? s : new Set(s).add(d.node.id)));
        // Open the tree down to the selection.
        const chain = d.ancestors.slice(0, -1).map((a) => a.id);
        for (const id of chain) await loadKids(id);
        setExpanded((s) => {
          if (chain.every((id) => id === ROOT_ID || s.has(id))) return s;
          const next = new Set(s);
          chain.forEach((id) => id !== ROOT_ID && next.add(id));
          return next;
        });
      })
      .catch(() => {});
    return () => {
      live = false;
    };
    // loadKids changes with every load; the selection alone decides this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [root, sel, version]);

  const onSelectRow = (n: NodeDto) => {
    setSel(n.id);
    if (n.isDir && n.hasChildren) {
      setZoom(n.id);
      loadKids(n.id).catch(() => {});
      setExpanded((s) => new Set(s).add(n.id));
    }
  };
  const onToggle = (n: NodeDto) => {
    setExpanded((s) => {
      const next = new Set(s);
      if (next.has(n.id)) next.delete(n.id);
      else next.add(n.id);
      return next;
    });
    loadKids(n.id).catch(() => {});
  };

  const appsUnlocked = useAppsUnlocked();
  const deleteSelected = useCallback(
    (permanent: boolean) => {
      const d = selDetail;
      if (!d || (d.protected && !(d.appFolder && appsUnlocked)) || d.node.id === ROOT_ID) return;
      const parentId = d.ancestors[d.ancestors.length - 2]?.id ?? ROOT_ID;
      const item = { root, id: d.node.id, name: d.node.name, path: d.path, size: d.node.size, appFolder: d.appFolder };
      confirmDelete([item], permanent, () => {
        setSel(parentId);
        if (zoom === d.node.id) setZoom(parentId);
      });
    },
    [selDetail, root, zoom, appsUnlocked],
  );

  // Delete moves the selection to the Recycle Bin (or deletes it, per Settings). Shift+Delete deletes permanently.
  const deleteMode = useStore((s) => s.deleteMode);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Delete" || e.target instanceof HTMLInputElement || useStore.getState().dialog) return;
      e.preventDefault();
      deleteSelected(e.shiftKey || deleteMode === "permanent");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [deleteSelected, deleteMode]);

  const isProtected = useCallback((id: number) => protectedIds.has(id), [protectedIds]);
  const crumbs = zoomDetail?.ancestors ?? [];
  const legend = zoomDetail ? sortedBreakdown(zoomDetail.breakdown) : [];

  return (
    <>
      <TreePane root={root} rootNode={rootNode} sel={sel} expanded={expanded} kids={kids} favIds={favIds} onSelect={onSelectRow} onToggle={onToggle} onStar={toggleStar} />
      <main className="main">
      <div className="mhead">
        <button
          className="btn ghost sm"
          aria-label={t("scan.up")}
          disabled={zoom === ROOT_ID}
          onClick={() => {
            const parent = crumbs[crumbs.length - 2];
            if (parent) {
              setSel(zoom);
              setZoom(parent.id);
            }
          }}
        >
          <Icon name="up" />
        </button>
        <div className="crumbs">
          {crumbs.map((c, i) => (
            <span key={c.id} className="row" style={{ gap: 2, flexWrap: "nowrap" }}>
              {i > 0 && <Icon name="chev" />}
              <button
                onClick={() => {
                  setZoom(c.id);
                  setSel(c.id);
                }}
              >
                {c.id === ROOT_ID ? rootLabel(root) : c.name}
              </button>
            </span>
          ))}
        </div>
        <div className="seg">
          <button className={view === "map" ? "on" : ""} onClick={() => setView("map")}>
            {t("explore.treemap")}
          </button>
          <button className={view === "types" ? "on" : ""} onClick={() => setView("types")}>
            {t("explore.types")}
          </button>
        </div>
        <button
          className="btn sm"
          disabled={!summary}
          title={t("export.report")}
          aria-label={t("export.report")}
          onClick={() =>
            summary &&
            scanReportHtml(summary)
              .then((html) => saveExport("html", `Lustro report ${rootLabel(root).replace(/[:\\/]/g, "")}.html`, html))
              .catch((e) => showToast(String(e), true))
          }
        >
          <Icon name="export" />
        </button>
        <button className="btn sm" title={t("drive.rescan")} aria-label={t("drive.rescan")} onClick={() => startScan(root)}>
          <Icon name="refresh" />
        </button>
      </div>
      {summary && (
        <div className="scanline">
          <span className="faint">{scanSummaryText(summary)}</span>
          {summary.denied > 0 && !isAdmin && (
            <span className="warn-text">
              {t("scan.deniedNote", { count: summary.denied })}{" "}
              <button className="linkbtn" onClick={requestAdmin}>
                {t("titlebar.runAsAdmin")}
              </button>
            </span>
          )}
        </div>
      )}
      {view === "map" ? (
        <>
          <div className="tmwrap">
            <Treemap
              data={map}
              selected={sel}
              isProtected={isProtected}
              onSelect={setSel}
              onZoom={(id) => {
                setZoom(id);
                setSel(id);
              }}
            />
          </div>
          <div className="legend">
            {legend.map(([k]) => (
              <span key={k} style={{ ["--c" as string]: kindColor(k) }}>
                <i />
                {t(`kind.${k}`)}
              </span>
            ))}
            <span style={{ flex: 1 }} />
            <span className="hint">{t("explore.hint")}</span>
          </div>
        </>
      ) : zoomDetail ? (
        <TypesView detail={zoomDetail} />
      ) : null}
      </main>
      <DetailPane
        root={root}
        detail={selDetail}
        rootSize={rootNode?.size ?? 0}
        starred={selDetail ? favIds.has(selDetail.node.id) : false}
        onDelete={deleteSelected}
        onStar={toggleStar}
      />
    </>
  );
}

export function Explore() {
  const { t } = useTranslation();
  const root = useStore((s) => s.exploreRoot);
  const scanning = useStore((s) => s.scanning);
  const summary = useStore((s) => (root ? s.scans[root] : undefined));
  const drives = useStore((s) => s.drives);
  const startScan = useStore((s) => s.startScan);
  const explore = useStore((s) => s.explore);

  if (scanning && (!root || scanning.root === root))
    return (
      <main className="main">
        <ScanProgressView root={scanning.root} />
      </main>
    );
  if (root && summary) return <ExploreView root={root} />;

  return (
    <main className="main">
    <div className="empty">
      <span className="ei">
        <Icon name="map" />
      </span>
      <h3>{root ? t("scan.notScannedTitle", { root: rootLabel(root) }) : t("scan.pickTitle")}</h3>
      <p>{root ? t("scan.notScannedBody") : t("scan.pickBody")}</p>
      <div className="row" style={{ justifyContent: "center" }}>
        {root ? (
          <button className="btn accent" onClick={() => startScan(root)}>
            {t("drive.scan")}
          </button>
        ) : (
          drives?.map((d) => (
            <button key={d.letter} className="btn" onClick={() => explore(d.root)}>
              <Icon name={driveIcon(d)} />
              {driveName(d)}
            </button>
          ))
        )}
      </div>
    </div>
    </main>
  );
}
