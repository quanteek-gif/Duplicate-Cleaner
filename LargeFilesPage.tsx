// Large files: top-100 biggest files with category filter and quick reveal.

import { useMemo, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowDownWideNarrow,
  ArrowUpWideNarrow,
  Copy as CopyIcon,
  FolderOpen,
  FolderSearch,
  Files,
  HardDrive,
  Search,
  Trash2,
} from "lucide-react";
import type { LargeFile, LargeFilesResult } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import {
  CATEGORY_COLORS,
  categoryLabel,
  fileKind,
  formatBytes,
  formatDate,
  formatNumber,
  percent,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  ContextMenu,
  EmptyState,
  SectionTitle,
  TextInput,
  useToast,
  type MenuItem,
} from "../components/ui";
import { DeletionDialog, type DeletionTarget } from "../components/DeletionDialog";

export function LargeFilesPage() {
  const toast = useToast();
  const { t } = useT();
  const [roots, setRoots] = useState<string[]>([]);
  const [manualPath, setManualPath] = useState("");
  const [result, setResult] = useState<LargeFilesResult | null>(null);
  const [running, setRunning] = useState(false);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string>("__all__");
  const [ascending, setAscending] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; file: LargeFile } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [deletionTarget, setDeletionTarget] = useState<DeletionTarget | null>(null);

  const addRoot = (path: string) =>
    setRoots((current) =>
      current.some((existing) => existing.toLowerCase() === path.toLowerCase()) ? current : [...current, path],
    );

  const pickFolder = async () => {
    if (!isTauri()) return;
    const picked = await openDialog({ directory: true, multiple: true });
    if (!picked) return;
    (Array.isArray(picked) ? picked : [picked]).forEach(addRoot);
  };

  const run = async () => {
    if (roots.length === 0) {
      toast.push("error", t("large.noLocations"), t("large.noLocationsBody"));
      return;
    }
    setRunning(true);
    try {
      const response = await api.scanLargeFiles(
        {
          paths: roots,
          excludedPaths: [],
          excludedDirNames: [],
          extensions: [],
          minSize: null,
          maxSize: null,
          followSymlinks: false,
          includeSystemDirs: false,
          useHashCache: false,
        },
        100,
      );
      setResult(response);
      setSelected(new Set());
      toast.push(
        "success",
        t("large.indexed"),
        t("large.indexedBody", {
          files: tc("file", response.scannedFiles),
          seconds: (response.elapsedMs / 1000).toFixed(1),
        }),
      );
    } catch (error) {
      toast.push("error", t("large.failed"), localizeBackend(errorMessage(error)));
    } finally {
      setRunning(false);
    }
  };

  const categories = useMemo(() => {
    if (!result) return [];
    return result.categoryTotals.map((total) => total.category);
  }, [result]);

  const files = useMemo(() => {
    if (!result) return [];
    let list = result.files.filter((file) => {
      if (category !== "__all__" && file.category !== category) return false;
      if (query && !file.name.toLowerCase().includes(query.toLowerCase()) && !file.path.toLowerCase().includes(query.toLowerCase()))
        return false;
      return true;
    });
    list = [...list].sort((a, b) => (ascending ? a.size - b.size : b.size - a.size));
    return list;
  }, [result, category, query, ascending]);

  const selectedBytes = (result?.files ?? [])
    .filter((file) => selected.has(file.path))
    .reduce((sum, file) => sum + file.size, 0);

  const contextItems = (file: LargeFile): MenuItem[] => [
    {
      label: t("dup.openFile"),
      icon: <FolderOpen size={14} />,
      onSelect: () => {
        void api.openInExplorer(file.path).catch((error: unknown) =>
          toast.push("error", t("dup.openFileFailed"), localizeBackend(errorMessage(error))),
        );
      },
    },
    {
      label: t("dup.openContaining"),
      icon: <FolderOpen size={14} />,
      onSelect: () => {
        void api.revealInFolder(file.path).catch((error: unknown) =>
          toast.push("error", t("dup.openFolderFailed"), localizeBackend(errorMessage(error))),
        );
      },
    },
    {
      label: t("dup.copyPath"),
      icon: <CopyIcon size={14} />,
      onSelect: () => {
        void navigator.clipboard.writeText(file.path);
      },
    },
    {
      label: t("dup.moveToBin"),
      icon: <Trash2 size={14} />,
      danger: true,
      onSelect: () => setDeletionTarget({ paths: [file.path], expectedHashes: {}, groupMembers: [], scanId: null }),
    },
  ];

  return (
    <div className="mx-auto max-w-[1180px] space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("large.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("large.subtitle")}
            {result && (
              <>
                {" "}
                {t("large.subtitleScanned", {
                  files: formatNumber(result.scannedFiles),
                  seconds: (result.elapsedMs / 1000).toFixed(1),
                })}
              </>
            )}
          </p>
        </div>
        <Button variant="primary" icon={<FolderSearch size={15} />} loading={running} onClick={() => void run()}>
          {t("large.find")}
        </Button>
      </div>

      <Card className="p-5">
        <SectionTitle
          title={t("large.locations")}
          subtitle={t("large.locationsSubtitle")}
          action={
            <Button size="sm" variant="secondary" icon={<FolderOpen size={13} />} onClick={() => void pickFolder()}>
              {t("large.addFolder")}
            </Button>
          }
        />
        <div className="flex flex-wrap gap-1.5">
          {roots.map((root) => (
            <span
              key={root}
              className="flex items-center gap-1.5 rounded-lg border border-line-soft bg-surface-1 px-2.5 py-1.5 text-[12px]"
            >
              <HardDrive size={12} className="text-text-tertiary" /> {root}
              <button className="text-text-tertiary hover:text-danger" onClick={() => setRoots((current) => current.filter((r) => r !== root))}>
                ×
              </button>
            </span>
          ))}
          <div className="flex flex-1 gap-2">
            <TextInput
              className="flex-1"
              placeholder={t("large.pathPlaceholder")}
              value={manualPath}
              onChange={(event) => setManualPath(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && manualPath.trim()) {
                  addRoot(manualPath.trim());
                  setManualPath("");
                }
              }}
            />
            <Button
              variant="subtle"
              onClick={() => {
                if (manualPath.trim()) {
                  addRoot(manualPath.trim());
                  setManualPath("");
                }
              }}
            >
              {t("common.add")}
            </Button>
          </div>
        </div>
      </Card>

      {!result && (
        <Card>
          <EmptyState
            icon={<Files size={26} />}
            title={t("large.emptyTitle")}
            description={t("large.emptyBody")}
          />
        </Card>
      )}

      {result && (
        <>
          {result.categoryTotals.length > 0 && (
            <Card className="p-5">
              <SectionTitle title={t("large.byCategory")} subtitle={t("large.byCategorySubtitle")} />
              <div className="space-y-3">
                {result.categoryTotals.map((total) => {
                  const share = percent(total.bytes, result.categoryTotals[0]?.bytes ?? 1);
                  return (
                    <button
                      key={total.category}
                      onClick={() => setCategory(category === total.category ? "__all__" : total.category)}
                      className={`w-full text-left ${category !== "__all__" && category !== total.category ? "opacity-50" : ""}`}
                    >
                      <div className="mb-1 flex items-center justify-between text-[12px]">
                        <span className="flex items-center gap-2 text-text-primary">
                          <span
                            className="h-2 w-2 rounded-full"
                            style={{ background: CATEGORY_COLORS[total.category] ?? "#6b7079" }}
                          />
                          {categoryLabel(total.category)}
                          <span className="text-text-tertiary">({formatNumber(total.count)})</span>
                        </span>
                        <span className="text-text-tertiary">{formatBytes(total.bytes)}</span>
                      </div>
                      <div className="h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
                        <div
                          className="h-full rounded-full transition-all"
                          style={{
                            width: `${share}%`,
                            background: CATEGORY_COLORS[total.category] ?? "#6b7079",
                          }}
                        />
                      </div>
                    </button>
                  );
                })}
              </div>
            </Card>
          )}

          <Card className="sticky top-0 z-20 flex flex-wrap items-center gap-2.5 p-3">
            <TextInput
              icon={<Search size={14} />}
              placeholder={t("large.filterPlaceholder")}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              className="w-[280px]"
            />
            <select
              value={category}
              onChange={(event) => setCategory(event.target.value)}
              className="h-9 rounded-[10px] border border-line bg-surface-1 px-2.5 text-[12.5px] text-text-primary outline-none"
            >
              <option value="__all__">{t("large.allCategories")}</option>
              {categories.map((value) => (
                <option key={value} value={value}>
                  {categoryLabel(value)}
                </option>
              ))}
            </select>
            <Button
              size="sm"
              variant="ghost"
              icon={ascending ? <ArrowUpWideNarrow size={14} /> : <ArrowDownWideNarrow size={14} />}
              onClick={() => setAscending((value) => !value)}
            >
              {ascending ? t("common.smallestFirst") : t("common.largestFirst")}
            </Button>
            <div className="ml-auto flex items-center gap-2">
              {selected.size > 0 && (
                <>
                  <span className="text-[12px] text-text-secondary">
                    {t("dup.selectedBar", {
                      n: formatNumber(selected.size),
                      size: formatBytes(selectedBytes),
                    })}
                  </span>
                  <Button
                    size="sm"
                    variant="danger"
                    icon={<Trash2 size={13} />}
                    onClick={() =>
                      setDeletionTarget({
                        paths: [...selected],
                        expectedHashes: {},
                        groupMembers: [],
                        scanId: null,
                      })
                    }
                  >
                    {t("dup.moveToBin")}
                  </Button>
                </>
              )}
            </div>
          </Card>

          {result.errors > 0 && (
            <div className="flex items-center gap-2 text-[12px] text-warning">
              <HardDrive size={12} />{" "}
              {t("dup.errorsSkipped", {
                items: tc("item", result.errors),
                verb: t("dup.errorsWasMany"),
              })}
            </div>
          )}

          <Card className="overflow-hidden pb-2">
            <table className="w-full border-collapse text-[12.5px]">
              <thead>
                <tr className="border-b border-line-soft text-left text-[10.5px] uppercase tracking-wider text-text-tertiary">
                  <th className="w-10 px-3 py-2.5" />
                  <th className="px-2 py-2.5 font-medium">{t("large.colName")}</th>
                  <th className="px-2 py-2.5 font-medium">{t("large.colCategory")}</th>
                  <th className="px-2 py-2.5 font-medium">{t("large.colModified")}</th>
                  <th className="px-3 py-2.5 text-right font-medium">{t("large.colSize")}</th>
                </tr>
              </thead>
              <tbody>
                {files.map((file) => (
                  <tr
                    key={file.path}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setMenu({ x: event.clientX, y: event.clientY, file });
                    }}
                    onDoubleClick={() =>
                      void api.revealInFolder(file.path).catch(() => undefined)
                    }
                    className={`border-b border-line-soft/60 transition-colors last:border-0 hover:bg-white/[0.03] ${
                      selected.has(file.path) ? "bg-danger/[0.05]" : ""
                    }`}
                  >
                    <td className="px-3 py-2">
                      <Checkbox
                        checked={selected.has(file.path)}
                        onChange={() =>
                          setSelected((current) => {
                            const next = new Set(current);
                            if (next.has(file.path)) next.delete(file.path);
                            else next.add(file.path);
                            return next;
                          })
                        }
                      />
                    </td>
                    <td className="max-w-0 px-2 py-2">
                      <div className="truncate text-text-primary" title={file.name}>
                        {file.name}
                      </div>
                      <div className="truncate text-[10.5px] text-text-tertiary" title={file.path}>
                        {file.path}
                      </div>
                    </td>
                    <td className="px-2 py-2">
                      <Badge>{categoryLabel(file.category)}</Badge>
                    </td>
                    <td className="whitespace-nowrap px-2 py-2 text-text-tertiary">
                      {formatDate(file.modifiedMs)}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right font-medium text-text-primary">
                      {formatBytes(file.size)}
                    </td>
                  </tr>
                ))}
                {files.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-3 py-10 text-center text-text-tertiary">
                      {t("large.noMatches")}
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
        </>
      )}

      {menu && <ContextMenu x={menu.x} y={menu.y} items={contextItems(menu.file)} onClose={() => setMenu(null)} />}
      <DeletionDialog
        open={deletionTarget !== null}
        target={deletionTarget}
        onClose={() => setDeletionTarget(null)}
        onCompleted={() => {
          setSelected(new Set());
          setResult((current) =>
            current
              ? { ...current, files: current.files.filter((file) => !selected.has(file.path)) }
              : current,
          );
        }}
      />

      <div className="flex items-center gap-2 text-[11px] text-text-tertiary">
        <Badge>{fileKind("exe")}</Badge> {t("large.systemNote")}
      </div>
    </div>
  );
}
