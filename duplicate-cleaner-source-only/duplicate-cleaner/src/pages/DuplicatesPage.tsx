// Duplicate results: groups with selectable files, auto-select extras,
// filters, sorting and a context menu with OS integration + deletion.

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowDownWideNarrow,
  ArrowUpWideNarrow,
  ChevronDown,
  ChevronRight,
  Copy as CopyIcon,
  CopyCheck,
  FileWarning,
  FolderOpen,
  Image as ImageIcon,
  Info,
  ListChecks,
  Recycle,
  Search,
  Sparkles,
  Trash2,
  Wand2,
} from "lucide-react";
import type {
  DeletionReport,
  DuplicateGroup,
  FileEntry,
  PageId,
  ScanResult,
} from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import {
  fileKind,
  formatBytes,
  formatDate,
  formatNumber,
  parentPath,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  ContextMenu,
  EmptyState,
  IconButton,
  KeyHint,
  TextInput,
  useToast,
  type MenuItem,
} from "../components/ui";
import { DeletionDialog, type DeletionTarget } from "../components/DeletionDialog";
import { useScan } from "../hooks/ScanContext";

type SortKey = "size" | "name" | "count" | "wasted";

/** Groups rendered before the user asks for more. */
const GROUP_PAGE_SIZE = 100;

export function DuplicatesPage({ onOpenScan }: { onNavigate: (page: PageId) => void; onOpenScan: () => void }) {
  const toast = useToast();
  const { t } = useT();
  const { result, loadScan, lastRecords, refreshRecent, applyDeletion } = useScan();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [extensionFilter, setExtensionFilter] = useState<string>("__all__");
  const [sortKey, setSortKey] = useState<SortKey>("wasted");
  const [ascending, setAscending] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; file: FileEntry; group: DuplicateGroup } | null>(null);
  const [deletionTarget, setDeletionTarget] = useState<DeletionTarget | null>(null);
  const [loadingScan, setLoadingScan] = useState<string | null>(null);
  const [errorsOpen, setErrorsOpen] = useState(false);

  const scan = result;
  const groups = scan?.groups ?? [];

  // Load the most recent scan automatically when there is no result in memory.
  const latestCompleted = lastRecords.find((record) => record.status === "completed");
  useEffect(() => {
    if (!scan && latestCompleted && isTauri()) {
      void loadScan(latestCompleted.scanId);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [latestCompleted?.scanId]);

  useEffect(() => {
    setSelected(new Set());
  }, [scan?.scanId]);

  const extensions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const group of groups) {
      for (const file of group.files) {
        const key = file.extension || "(none)";
        counts.set(key, (counts.get(key) ?? 0) + 1);
      }
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [groups]);

  const filteredGroups = useMemo(() => {
    const text = query.trim().toLowerCase();
    let list = groups.filter((group) => {
      if (text) {
        const inGroup =
          group.hash.startsWith(text) ||
          group.files.some(
            (file) =>
              file.name.toLowerCase().includes(text) ||
              file.path.toLowerCase().includes(text),
          );
        if (!inGroup) return false;
      }
      if (extensionFilter !== "__all__") {
        const extension = extensionFilter === "(none)" ? "" : extensionFilter;
        if (!group.files.some((file) => file.extension === extension)) return false;
      }
      return true;
    });

    // `ascending === false` means "largest first" — the default.
    const direction = ascending ? 1 : -1;
    list = [...list].sort((a, b) => {
      switch (sortKey) {
        case "size":
          return (a.size - b.size) * direction;
        case "name":
          return (a.files[0]?.name ?? "").localeCompare(b.files[0]?.name ?? "") * -direction;
        case "count":
          return (a.files.length - b.files.length) * direction;
        default:
          return (a.wastedBytes - b.wastedBytes) * direction;
      }
    });
    return list;
  }, [groups, query, extensionFilter, sortKey, ascending]);

  const filtering = query.trim() !== "" || extensionFilter !== "__all__";
  const filteredExtras = useMemo(
    () => filteredGroups.reduce((sum, group) => sum + Math.max(0, group.files.length - 1), 0),
    [filteredGroups],
  );
  const filteredWasted = useMemo(
    () => filteredGroups.reduce((sum, group) => sum + group.wastedBytes, 0),
    [filteredGroups],
  );

  // Rendering every group at once is what makes a scan of a real volume hurt:
  // 2 500 groups with several rows each is tens of thousands of DOM nodes and
  // the webview ballooned past a gigabyte. Show a page at a time instead; the
  // count is rendered cheaply and "show more" is explicit.
  const [visibleGroups, setVisibleGroups] = useState(GROUP_PAGE_SIZE);
  useEffect(() => {
    setVisibleGroups(GROUP_PAGE_SIZE);
  }, [filteredGroups.length]);

  const shownGroups = useMemo(
    () => filteredGroups.slice(0, visibleGroups),
    [filteredGroups, visibleGroups],
  );
  const hiddenGroupCount = filteredGroups.length - shownGroups.length;

  const selectedFiles = useMemo(() => {
    const list: { file: FileEntry; group: DuplicateGroup }[] = [];
    for (const group of groups) {
      for (const file of group.files) {
        if (selected.has(file.path)) list.push({ file, group });
      }
    }
    return list;
  }, [groups, selected]);

  const selectedBytes = selectedFiles.reduce((sum, item) => sum + item.file.size, 0);

  const toggleFile = useCallback((path: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const toggleGroup = useCallback((group: DuplicateGroup) => {
    setSelected((current) => {
      const next = new Set(current);
      const allSelected = group.files.every((file) => next.has(file.path));
      for (const file of group.files) {
        if (allSelected) next.delete(file.path);
        else next.add(file.path);
      }
      return next;
    });
  }, []);

  /**
   * Select every redundant copy. The copy to keep is decided in Rust
   * (`core::analyzer`) — the one in a working folder, otherwise the newest —
   * so a stray copy in a backup folder is not the one that survives.
   */
  const autoSelectExtras = useCallback(() => {
    const next = new Set<string>();
    let fallbackGroups = 0;
    for (const group of groups) {
      if (group.files.length < 2) continue;
      const keep = group.recommendedKeep?.path;
      if (!keep) fallbackGroups += 1;
      const keeper = keep ?? [...group.files].sort((a, b) => a.path.localeCompare(b.path))[0]?.path;
      for (const file of group.files) {
        if (file.path !== keeper) next.add(file.path);
      }
    }
    setSelected(next);
    const total = next.size;
    toast.push(
      "info",
      t("dup.autoSelectedTitle", { copies: tc("redundantCopy", total) }),
      fallbackGroups > 0
        ? t("dup.autoSelectedRanked", {
            groups: tc("group", groups.length - fallbackGroups),
            fallbacks: formatNumber(fallbackGroups),
          })
        : t("dup.autoSelectedBody"),
    );
  }, [groups, toast]);

  const clearSelection = () => setSelected(new Set());

  const openDeletion = (paths: string[]) => {
    if (!scan || paths.length === 0) return;
    const hashByPath: Record<string, string> = {};
    const groupMembers: string[][] = [];
    for (const group of groups) {
      const memberPaths: string[] = [];
      for (const file of group.files) {
        hashByPath[file.path] = group.hash;
        memberPaths.push(file.path);
      }
      groupMembers.push(memberPaths);
    }
    const filtered = Object.fromEntries(
      Object.entries(hashByPath).filter(([path]) => paths.includes(path)),
    );
    setDeletionTarget({
      paths,
      expectedHashes: filtered,
      groupMembers,
      scanId: scan.scanId,
    });
  };

  const onDeleted = async (report: DeletionReport) => {
    setSelected(new Set());
    const removed = report.outcomes
      .filter((outcome) => outcome.success)
      .map((outcome) => outcome.path);
    if (removed.length > 0) applyDeletion(removed);
    await refreshRecent();
  };

  const contextItems = (file: FileEntry, group: DuplicateGroup): MenuItem[] => [
    {
      label: t("dup.openFile"),
      icon: <FolderOpen size={14} />,
      onSelect: () => void runAction(() => api.openInExplorer(file.path), t("dup.openFileFailed")),
    },
    {
      label: t("dup.openContaining"),
      icon: <FolderOpen size={14} />,
      onSelect: () => void runAction(() => api.revealInFolder(file.path), t("dup.openFolderFailed")),
    },
    {
      label: t("dup.copyPath"),
      icon: <CopyIcon size={14} />,
      onSelect: () => {
        void navigator.clipboard
          .writeText(file.path)
          .then(() => toast.push("success", t("common.pathCopied")))
          .catch(() => toast.push("error", t("common.clipboardUnavailable")));
      },
    },
    {
      label: selected.has(file.path) ? t("dup.removeFromSelection") : t("dup.selectForDeletion"),
      icon: <ListChecks size={14} />,
      onSelect: () => toggleFile(file.path),
    },
    {
      label: t("dup.keepOnlyThis"),
      icon: <Sparkles size={14} />,
      onSelect: () => {
        const next = new Set<string>();
        for (const other of group.files) {
          if (other.path !== file.path) next.add(other.path);
        }
        setSelected(next);
      },
    },
    {
      label: t("dup.moveToBin"),
      icon: <Trash2 size={14} />,
      danger: true,
      onSelect: () => openDeletion([file.path]),
    },
  ];

  const runAction = async (action: () => Promise<void>, errorTitle: string) => {
    if (!isTauri()) {
      toast.push("error", t("common.desktopOnly"), t("common.desktopOnlyBody"));
      return;
    }
    try {
      await action();
    } catch (error) {
      toast.push("error", errorTitle, localizeBackend(errorMessage(error)));
    }
  };

  // ------------------------------------------------------------- render
  if (!scan || groups.length === 0) {
    return (
      <div className="mx-auto max-w-[1180px]">
        <PageHeader total={0} groups={0} onOpenScan={onOpenScan} />
        <Card className="mt-6">
          <EmptyState
            icon={<CopyCheck size={26} />}
            title={scan ? t("dup.emptyNoDuplicates") : t("dup.emptyNoResults")}
            description={
              scan ? t("dup.emptyNoDuplicatesBody") : t("dup.emptyNoResultsBody")
            }
            action={
              <Button variant="primary" icon={<Wand2 size={15} />} onClick={onOpenScan}>
                {scan ? t("dup.scanAgain") : t("dup.startScan")}
              </Button>
            }
          />
        </Card>
        {lastRecords.length > 0 && (
          <RecentScans
            records={lastRecords}
            loadingScan={loadingScan}
            onLoad={async (scanId) => {
              setLoadingScan(scanId);
              await loadScan(scanId);
              setLoadingScan(null);
            }}
          />
        )}
      </div>
    );
  }

  const allPaths = groups.flatMap((group) => group.files.map((file) => file.path));
  const everythingSelected = allPaths.length > 0 && allPaths.every((path) => selected.has(path));

  return (
    <div className="mx-auto max-w-[1180px] space-y-5">
      <PageHeader
        total={scan.stats.duplicateFiles}
        groups={scan.stats.duplicateGroups}
        wastedBytes={scan.stats.wastedBytes}
        filtered={
          filtering
            ? {
                total: filteredExtras,
                groups: filteredGroups.length,
                wastedBytes: filteredWasted,
              }
            : undefined
        }
        onOpenScan={onOpenScan}
      />

      {scan.errors.length > 0 && (
        <Card className="p-0">
          <button
            type="button"
            onClick={() => setErrorsOpen((open) => !open)}
            className="flex w-full items-center gap-2.5 px-4 py-3 text-left text-[12.5px] text-text-secondary transition-colors hover:bg-white/[0.03]"
          >
            <FileWarning size={14} className="shrink-0 text-warning" />
            <span>
              {t("dup.errorsSkipped", {
                items: tc("item", scan.errors.length),
                verb: t(scan.errors.length === 1 ? "dup.errorsWasOne" : "dup.errorsWasMany"),
              })}
            </span>
            <span className="ml-auto text-text-tertiary">
              {errorsOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </span>
          </button>
          {errorsOpen && (
            <div className="max-h-56 overflow-auto border-t border-line px-4 py-2">
              {scan.errors.map((entry) => (
                <div key={entry.path} className="py-1 text-[11.5px]">
                  <div className="truncate font-mono text-text-tertiary" title={entry.path}>
                    {entry.path}
                  </div>
                  <div className="text-text-tertiary/80">{localizeBackend(entry.message)}</div>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}

      {/* toolbar */}
      <Card className="sticky top-0 z-20 flex flex-wrap items-center gap-2.5 p-3">
        <TextInput
          icon={<Search size={14} />}
          placeholder={t("dup.searchPlaceholder")}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          className="w-[280px]"
        />
        <select
          value={extensionFilter}
          onChange={(event) => setExtensionFilter(event.target.value)}
          className="h-9 rounded-[10px] border border-line bg-surface-1 px-2.5 text-[12.5px] text-text-primary outline-none focus:border-accent/60"
        >
          <option value="__all__">{t("dup.allTypes", { groups: tc("group", groups.length) })}</option>
          {extensions.map(([extension, count]) => (
            <option key={extension} value={extension}>
              {fileKind(extension === "(none)" ? "" : extension)} ({count})
            </option>
          ))}
        </select>

        <select
          value={sortKey}
          onChange={(event) => setSortKey(event.target.value as SortKey)}
          className="h-9 rounded-[10px] border border-line bg-surface-1 px-2.5 text-[12.5px] text-text-primary outline-none focus:border-accent/60"
        >
          <option value="wasted">{t("dup.sortWasted")}</option>
          <option value="size">{t("dup.sortSize")}</option>
          <option value="count">{t("dup.sortCount")}</option>
          <option value="name">{t("dup.sortName")}</option>
        </select>
        <IconButton
          label={ascending ? t("common.descending") : t("common.ascending")}
          onClick={() => setAscending((value) => !value)}
          active
        >
          {ascending ? <ArrowUpWideNarrow size={15} /> : <ArrowDownWideNarrow size={15} />}
        </IconButton>

        <div className="ml-auto flex items-center gap-2">
          <Checkbox
            checked={everythingSelected}
            indeterminate={!everythingSelected && selected.size > 0}
            onChange={(checked) =>
              setSelected(checked ? new Set(allPaths) : new Set())
            }
            label={t("common.all")}
          />
          <Button size="sm" variant="subtle" icon={<Wand2 size={13} />} onClick={autoSelectExtras}>
            {t("dup.autoSelect")}
          </Button>
          {selected.size > 0 && (
            <Button
              size="sm"
              variant="danger"
              icon={<Recycle size={13} />}
              onClick={() => openDeletion([...selected])}
            >
              {t("dup.deleteSelected", {
                n: formatNumber(selected.size),
                size: formatBytes(selectedBytes),
              })}
            </Button>
          )}
        </div>
      </Card>

      {/* groups */}
      <div className="space-y-3 pb-24">
        {filteredGroups.length === 0 && (
          <Card className="p-8 text-center text-[13px] text-text-tertiary">
            {t("dup.noMatches")}
          </Card>
        )}
        {shownGroups.map((group, index) => {
          const isCollapsed = collapsed.has(group.hash);
          const groupSelected = group.files.filter((file) => selected.has(file.path)).length;
          return (
            <Card key={`${group.hash}-${index}`} className="overflow-hidden">
              <div className="flex items-center gap-3 px-4 py-3">
                <IconButton
                  label={isCollapsed ? t("dup.expand") : t("dup.collapse")}
                  onClick={() =>
                    setCollapsed((current) => {
                      const next = new Set(current);
                      if (next.has(group.hash)) next.delete(group.hash);
                      else next.add(group.hash);
                      return next;
                    })
                  }
                >
                  {isCollapsed ? <ChevronRight size={15} /> : <ChevronDown size={15} />}
                </IconButton>
                <Checkbox
                  checked={groupSelected === group.files.length}
                  indeterminate={groupSelected > 0 && groupSelected < group.files.length}
                  onChange={() => toggleGroup(group)}
                />
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="truncate text-[13.5px] font-medium text-text-primary">
                      {group.files[0]?.name ?? t("dup.unnamed")}
                    </span>
                    <Badge tone="accent">{tc("copy", group.files.length)}</Badge>
                    <Badge>{fileKind(group.files[0]?.extension ?? "")}</Badge>
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-2 text-[11px] text-text-tertiary">
                    <span>{t("dup.each", { size: formatBytes(group.size) })}</span>
                    <span>·</span>
                    <span className="text-success">
                      {t("dup.reclaimable", { size: formatBytes(group.wastedBytes) })}
                    </span>
                    <span>·</span>
                    <span className="font-mono" title={`SHA-256: ${group.hash}`}>
                      {group.hash.slice(0, 12)}…
                    </span>
                  </div>
                </div>
                {groupSelected > 0 && (
                  <Badge tone="warning">
                    {t("common.selected", { n: formatNumber(groupSelected) })}
                  </Badge>
                )}
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Recycle size={13} />}
                  onClick={() => {
                    // Keep the copy the Rust analyzer picked, review the rest.
                    const keepPath =
                      group.recommendedKeep?.path ?? group.files[0]?.path;
                    const extras = group.files
                      .map((file) => file.path)
                      .filter((path) => path !== keepPath);
                    openDeletion(extras);
                  }}
                >
                  {t("dup.keepRecommended")}
                </Button>
              </div>

              {!isCollapsed && (
                <div className="border-t border-line-soft">
                  {group.files.map((file) => {
                    const isSelected = selected.has(file.path);
                    const keep = group.recommendedKeep;
                    const isKeep = keep?.path === file.path;
                    return (
                      <div
                        key={file.path}
                        onContextMenu={(event) => {
                          event.preventDefault();
                          setMenu({ x: event.clientX, y: event.clientY, file, group });
                        }}
                        onDoubleClick={() =>
                          void runAction(() => api.revealInFolder(file.path), t("dup.openFolderFailed"))
                        }
                        className={`flex cursor-default items-center gap-3 px-4 py-2.5 transition-colors hover:bg-white/[0.03] ${
                          isSelected ? "bg-danger/[0.06]" : ""
                        }`}
                      >
                        <Checkbox checked={isSelected} onChange={() => toggleFile(file.path)} />
                        <FileTypeIcon extension={file.extension} />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="truncate text-[12.5px] text-text-primary">{file.name}</span>
                            {isKeep && !isSelected && (
                              <Badge
                                tone="success"
                                title={keep ? t("dup.suggested", { explanation: localizeBackend(keep.explanation) }) : undefined}
                              >
                                {t("dup.keep")}
                              </Badge>
                            )}
                          </div>
                          <div className="truncate text-[11px] text-text-tertiary" title={file.path}>
                            {file.path}
                          </div>
                        </div>
                        <div className="hidden shrink-0 text-right md:block">
                          <div className="text-[11.5px] text-text-secondary">{formatBytes(file.size)}</div>
                          <div className="text-[10.5px] text-text-tertiary">
                            {t("dup.modified", { date: formatDate(file.modifiedMs) })}
                          </div>
                          <div className="text-[10.5px] text-text-tertiary/80">
                            {t("dup.created", { date: formatDate(file.createdMs) })}
                          </div>
                        </div>
                        <IconButton
                          label={t("dup.openFolder")}
                          onClick={() =>
                            void runAction(() => api.revealInFolder(file.path), t("dup.openFolderFailed"))
                          }
                        >
                          <FolderOpen size={14} />
                        </IconButton>
                      </div>
                    );
                  })}
                </div>
              )}
            </Card>
          );
        })}
        {filteredGroups.length > 0 && hiddenGroupCount === 0 && filteredGroups.length > GROUP_PAGE_SIZE && (
          <p className="pt-1 text-center text-[11.5px] text-text-tertiary">
            {t("dup.endOfResults", { groups: tc("group", filteredGroups.length) })}
          </p>
        )}
        {hiddenGroupCount > 0 && (
          <Card className="flex items-center justify-between gap-3 px-4 py-3">
            <span className="text-[12.5px] text-text-tertiary">
              {t("dup.showing", {
                shown: formatNumber(shownGroups.length),
                total: tc("group", filteredGroups.length),
              })}
            </span>
            <div className="flex gap-2">
              <Button size="sm" variant="subtle" onClick={() => setVisibleGroups((n) => n + GROUP_PAGE_SIZE)}>
                {t("common.showMore", { n: formatNumber(Math.min(GROUP_PAGE_SIZE, hiddenGroupCount)) })}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setVisibleGroups(filteredGroups.length)}>
                {t("common.showAll")}
              </Button>
            </div>
          </Card>
        )}
      </div>

      {/* floating action bar */}
      {selected.size > 0 && (
        <div className="fixed bottom-5 left-1/2 z-30 flex -translate-x-1/2 items-center gap-3 rounded-2xl border border-line bg-surface-2/95 px-4 py-2.5 shadow-2xl backdrop-blur-xl animate-slide-up">
          <span className="text-[12.5px] text-text-secondary">
            <span className="font-semibold text-text-primary">{formatNumber(selected.size)}</span> selected ·{" "}
            {formatBytes(selectedBytes)}
          </span>
          <Button size="sm" variant="ghost" onClick={clearSelection}>
            {t("common.clear")}
          </Button>
          <Button size="sm" variant="danger" icon={<Recycle size={13} />} onClick={() => openDeletion([...selected])}>
            {t("dup.moveToBin")}
          </Button>
        </div>
      )}

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={contextItems(menu.file, menu.group)}
          onClose={() => setMenu(null)}
        />
      )}

      <DeletionDialog
        open={deletionTarget !== null}
        target={deletionTarget}
        onClose={() => setDeletionTarget(null)}
        onCompleted={(report) => void onDeleted(report)}
      />
    </div>
  );
}

function PageHeader({
  total,
  groups,
  wastedBytes,
  filtered,
  onOpenScan,
}: {
  total: number;
  groups: number;
  wastedBytes?: number;
  /** Present while a search or type filter narrows the list: the headline then
      describes what is on screen, not the whole scan. */
  filtered?: { total: number; groups: number; wastedBytes: number };
  onOpenScan: () => void;
}) {
  const { t } = useT();
  const shown = filtered ?? { total, groups, wastedBytes: wastedBytes ?? 0 };
  const size = formatBytes(shown.wastedBytes);
  const line =
    groups > 0
      ? filtered
        ? t("dup.headerFiltered", {
            files: tc("duplicateFile", shown.total),
            groups: tc("group", shown.groups),
            size,
            totalFiles: tc("duplicateFile", total),
            totalGroups: tc("group", groups),
          })
        : t("dup.header", {
            files: tc("duplicateFile", shown.total),
            groups: tc("group", shown.groups),
            size,
          })
      : t("dup.subtitleEmpty");
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("dup.title")}</h1>
        <p className="mt-1 text-[13px] text-text-secondary">
          {highlight(line, size, "text-success")}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <span className="hidden items-center gap-1 text-[11px] text-text-tertiary lg:flex">
          <KeyHint>{t("dup.rightClick")}</KeyHint> {t("dup.rightClickHint")}
        </span>
        <Button variant="secondary" onClick={onOpenScan}>
          {t("dup.newScan")}
        </Button>
      </div>
    </div>
  );
}

/**
 * Wrap the first occurrence of `needle` in a styled span. Sentences are built
 * from templates, so the emphasised fragment has to be found inside the
 * finished string instead of being spliced into the JSX — otherwise every
 * language would need its own markup.
 */
function highlight(text: string, needle: string, className: string) {
  const index = needle ? text.indexOf(needle) : -1;
  if (index < 0) return text;
  return (
    <>
      {text.slice(0, index)}
      <span className={className}>{needle}</span>
      {text.slice(index + needle.length)}
    </>
  );
}

function RecentScans({
  records,
  loadingScan,
  onLoad,
}: {
  records: { scanId: string; startedAt: number; filesScanned: number; duplicateFiles: number }[];
  loadingScan: string | null;
  onLoad: (scanId: string) => Promise<void>;
}) {
  const { t } = useT();
  return (
    <Card className="mt-5 p-5">
      <div className="mb-3 flex items-center gap-2 text-[13px] font-medium text-text-primary">
        <Info size={14} className="text-text-tertiary" /> {t("dup.previousScans")}
      </div>
      <div className="space-y-1">
        {records.slice(0, 6).map((record) => (
          <div key={record.scanId} className="flex items-center gap-3 rounded-lg px-2 py-2 hover:bg-white/[0.03]">
            <span className="min-w-0 flex-1 truncate text-[12px] text-text-secondary">
              {formatDate(record.startedAt)} · {tc("file", record.filesScanned)} ·{" "}
              {tc("duplicateFile", record.duplicateFiles)}
            </span>
            <Button
              size="sm"
              variant="ghost"
              loading={loadingScan === record.scanId}
              onClick={() => void onLoad(record.scanId)}
            >
              {t("common.open")}
            </Button>
          </div>
        ))}
      </div>
    </Card>
  );
}

const TYPE_COLORS: Record<string, string> = {
  img: "bg-[#4f8cff]/15 text-[#4f8cff]",
  vid: "bg-[#ff7a45]/15 text-[#ff7a45]",
  aud: "bg-[#c084fc]/15 text-[#c084fc]",
  doc: "bg-[#3ecf8e]/15 text-[#3ecf8e]",
  arc: "bg-[#f5b533]/15 text-[#f5b533]",
  exe: "bg-[#f56c6c]/15 text-[#f56c6c]",
  cod: "bg-[#38c7f5]/15 text-[#38c7f5]",
  oth: "bg-white/[0.07] text-text-tertiary",
};

function FileTypeIcon({ extension }: { extension: string }) {
  const { t } = useT();
  const short = extension.slice(0, 3).padEnd(3, " ");
  const key = /jpe?g|png|gif|bmp|webp|tiff?/.test(extension)
    ? "img"
    : /mp4|mkv|avi|mov|webm/.test(extension)
      ? "vid"
      : /mp3|wav|flac|ogg|m4a/.test(extension)
        ? "aud"
        : /pdf|docx?|xlsx?|pptx?|txt|md/.test(extension)
          ? "doc"
          : /zip|rar|7z|tar|gz/.test(extension)
            ? "arc"
            : /exe|msi|dll/.test(extension)
              ? "exe"
              : /rs|tsx?|jsx?|py|c|cpp|h|go|java/.test(extension)
                ? "cod"
                : "oth";
  return (
    <span
      className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-lg font-mono text-[9px] font-semibold uppercase ${TYPE_COLORS[key]}`}
      title={extension || t("dup.fileType")}
    >
      {extension ? short.toUpperCase() : <FileWarning size={13} />}
    </span>
  );
}

export function GroupsSummary({ scan }: { scan: ScanResult | null }) {
  if (!scan) return null;
  return (
    <div className="flex items-center gap-2 text-[12px] text-text-tertiary">
      <ImageIcon size={12} /> {tc("group", scan.groups.length)} ·{" "}
      {tc("file", scan.stats.duplicateFiles)}
      {scan.groups[0] && <span>· {parentPath(scan.groups[0].files[0]?.path ?? "")}</span>}
    </div>
  );
}
