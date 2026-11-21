// Similar photos: perceptual-hash groups with side-by-side previews.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  Eye,
  FolderOpen,
  ImageOff,
  Images,
  Loader2,
  Maximize2,
  Plus,
  Recycle,
  Sparkles,
  X,
} from "lucide-react";
import type { SimilarGroup, SimilarImage, SimilarPhotosResult, SimilarProgress } from "../types";
import { usePersistentPaths, addUniquePath } from "../hooks/usePersistentPaths";
import { api, errorMessage, isTauri, onSimilarProgress } from "../services/api";
import {
  MAX_SIMILAR_THRESHOLD,
  formatBytes,
  formatNumber,
  percent,
} from "../services/format";
import { localizeBackend, tc, useT } from "../i18n";
import { DeletionDialog, type DeletionTarget } from "../components/DeletionDialog";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  EmptyState,
  Modal,
  ProgressBar,
  SectionTitle,
  TextInput,
  useToast,
} from "../components/ui";

/** The largest member of a group carries the most detail — use it as the visual reference. */
function referenceOf(group: SimilarGroup): SimilarImage {
  return group.images.reduce((best, image) =>
    image.file.size > best.file.size ? image : best,
  );
}

/** Photo groups rendered before the user asks for more. */
const SIMILAR_GROUP_PAGE_SIZE = 20;

export function SimilarPhotosPage() {
  const toast = useToast();
  const { t } = useT();
  const [paths, setPaths] = usePersistentPaths("duplicate-cleaner.similar-folders");
  const [manualPath, setManualPath] = useState("");
  const [threshold, setThreshold] = useState(8);
  const [result, setResult] = useState<SimilarPhotosResult | null>(null);
  const [progress, setProgress] = useState<SimilarProgress | null>(null);
  const [running, setRunning] = useState(false);
  // Side-by-side comparison: the reference image plus the copy being compared.
  const [compare, setCompare] = useState<{ group: SimilarGroup; index: number } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [quickFolders, setQuickFolders] = useState<{ label: string; path: string }[]>([]);
  // Same reasoning as the Duplicates page: a library can produce hundreds of
  // photo groups, each holding up to 60 base64 previews. Rendering them all
  // at once exhausted the webview; page them instead.
  const [visibleGroups, setVisibleGroups] = useState(SIMILAR_GROUP_PAGE_SIZE);
  useEffect(() => {
    setVisibleGroups(SIMILAR_GROUP_PAGE_SIZE);
  }, [result?.groups.length]);
  const shownGroups = useMemo(
    () => (result?.groups ?? []).slice(0, visibleGroups),
    [result, visibleGroups],
  );
  const hiddenGroupCount = (result?.groups.length ?? 0) - shownGroups.length;

  const [deletionTarget, setDeletionTarget] = useState<DeletionTarget | null>(null);
  const previewCache = useRef<Map<string, string>>(new Map());

  useEffect(() => {
    if (!isTauri()) return;
    void api.getQuickFolders().then(setQuickFolders).catch(() => undefined);
    void api
      .getSettings()
      .then((settings) => setThreshold(settings.similarPhotosThreshold))
      .catch(() => undefined);
    let dispose: (() => void) | undefined;
    void onSimilarProgress(setProgress).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, []);

  const addPath = (path: string) => setPaths((current) => addUniquePath(current, path));

  const pickFolder = async () => {
    if (!isTauri()) return;
    const selectedPath = await openDialog({ directory: true, multiple: true });
    if (!selectedPath) return;
    (Array.isArray(selectedPath) ? selectedPath : [selectedPath]).forEach(addPath);
  };

  const scan = async () => {
    if (paths.length === 0) {
      toast.push("error", t("sim.noFolders"), t("sim.noFoldersBody"));
      return;
    }
    setRunning(true);
    setProgress(null);
    setSelected(new Set());
    previewCache.current.clear();
    try {
      const analysis = await api.findSimilarPhotos(paths, threshold);
      setResult(analysis);
      toast.push(
        "success",
        t("sim.groupsFoundTitle", { n: formatNumber(analysis.groups.length) }),
        t("sim.groupsFoundBody", {
          images: formatNumber(analysis.imagesScanned),
          seconds: (analysis.elapsedMs / 1000).toFixed(1),
        }),
      );
    } catch (error) {
      const message = errorMessage(error);
      if (!message.toLowerCase().includes("cancel")) {
        toast.push("error", t("sim.analysisFailed"), localizeBackend(message));
      }
    } finally {
      setRunning(false);
      setProgress(null);
    }
  };

  /** Load a full-resolution preview for the comparison view (cached). */
  const loadFull = useCallback(async (path: string): Promise<string | null> => {
    const cached = previewCache.current.get(path);
    if (cached) return cached;
    if (!isTauri()) return null;
    try {
      const full = await api.getImagePreview(path, 1024);
      previewCache.current.set(path, full);
      return full;
    } catch {
      return null;
    }
  }, []);

  /**
   * Open the side-by-side view for `image` inside its group. The largest file
   * of the group is used as the visual reference, because it carries the most
   * detail; the clicked image is compared against it.
   */
  const openCompare = (group: SimilarGroup, image: SimilarImage) => {
    const reference = referenceOf(group);
    const index = image.file.path === reference.file.path ? 0 : group.images.indexOf(image);
    setCompare({ group, index: Math.max(0, index) });
  };

  /** Open the verified deletion dialog for the given image paths. */
  const openDeletionForImages = (paths: string[]) => {
    if (paths.length === 0 || !result) return;
    const groupMembers: string[][] = [];
    for (const group of result.groups) {
      groupMembers.push(group.images.map((image) => image.file.path));
    }
    setDeletionTarget({
      paths,
      // pHash values are not SHA-256; deletion re-verification uses the file
      // existence + last-copy guards instead of content hashes here.
      expectedHashes: {},
      groupMembers,
      scanId: null,
    });
  };

  const totalWasted = useMemo(
    () => result?.groups.reduce((sum, group) => sum + groupWasted(group), 0) ?? 0,
    [result],
  );

  return (
    <div className="mx-auto max-w-[1180px] space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("sim.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("sim.subtitle")}
            {result && (
              <>
                {" "}
                {t("sim.resultSummary", {
                  groups: tc("group", result.groups.length),
                  size: formatBytes(totalWasted),
                })}
              </>
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {result && result.groups.length > 0 && (
            <Button
              variant="danger"
              icon={<Recycle size={14} />}
              disabled={selected.size === 0}
              onClick={() => openDeletionForImages([...selected])}
            >
              {selected.size > 0
                ? t("sim.recycleSelected", { n: formatNumber(selected.size) })
                : t("sim.selectCopies")}
            </Button>
          )}
          <Button variant="primary" icon={<Sparkles size={15} />} loading={running} onClick={() => void scan()}>
            {t("sim.analyze")}
          </Button>
        </div>
      </div>

      <Card className="p-5">
        <SectionTitle
          title={t("sim.folders")}
          subtitle={t("sim.foldersSubtitle")}
          action={
            <Button size="sm" variant="secondary" icon={<FolderOpen size={13} />} onClick={() => void pickFolder()}>
              {t("sim.addFolder")}
            </Button>
          }
        />
        <div className="flex flex-wrap gap-1.5">
          {paths.map((path) => (
            <span
              key={path}
              className="flex items-center gap-1.5 rounded-lg border border-line-soft bg-surface-1 px-2.5 py-1.5 text-[12px] text-text-primary"
            >
              {path}
              <button
                className="text-text-tertiary hover:text-danger"
                onClick={() => setPaths((current) => current.filter((p) => p !== path))}
              >
                <X size={12} />
              </button>
            </span>
          ))}
          {quickFolders
            .filter((folder) => folder.label === "Pictures")
            .map((folder) => (
              <Button key={folder.path} size="sm" variant="ghost" icon={<Plus size={12} />} onClick={() => addPath(folder.path)}>
                {folder.label}
              </Button>
            ))}
        </div>
        <div className="mt-3 flex gap-2">
          <TextInput
            className="flex-1"
            placeholder={t("sim.pathPlaceholder")}
            value={manualPath}
            onChange={(event) => setManualPath(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && manualPath.trim()) {
                addPath(manualPath.trim());
                setManualPath("");
              }
            }}
          />
          <Button
            variant="subtle"
            onClick={() => {
              if (manualPath.trim()) {
                addPath(manualPath.trim());
                setManualPath("");
              }
            }}
          >
            {t("common.add")}
          </Button>
        </div>

        <div className="mt-4">
          <div className="mb-1.5 flex items-center justify-between text-[12px] text-text-secondary">
            <span>{t("sim.sensitivity")}</span>
            <span className="font-mono text-text-primary">{t("sim.bits", { n: formatNumber(threshold) })}</span>
          </div>
          <input
            type="range"
            min={0}
            max={MAX_SIMILAR_THRESHOLD}
            value={threshold}
            onChange={(event) => setThreshold(Number(event.target.value))}
            className="w-full accent-[#4f8cff]"
          />
          <div className="mt-1 flex justify-between text-[10.5px] text-text-tertiary">
            <span>{t("sim.strict")}</span>
            <span>{t("sim.loose")}</span>
          </div>
        </div>
      </Card>

      {running && progress && (
        <Card className="p-5">
          <div className="flex items-center justify-between text-[13px]">
            <span className="flex items-center gap-2 text-text-primary">
              <Loader2 size={14} className="animate-spin text-accent" />
              {progress.phase === "discovering" ? t("sim.discovering") : t("sim.hashing")}
            </span>
            <span className="text-text-tertiary">
              {formatNumber(progress.imagesHashed)} / {formatNumber(progress.imagesToHash || progress.imagesFound)}
            </span>
          </div>
          <ProgressBar
            className="mt-3"
            value={
              progress.imagesToHash > 0
                ? percent(progress.imagesHashed, progress.imagesToHash)
                : progress.phase === "discovering"
                  ? 5
                  : 0
            }
            indeterminate={progress.phase === "discovering"}
          />
        </Card>
      )}

      {!result && !running && (
        <Card>
          <EmptyState
            icon={<Images size={26} />}
            title={t("sim.emptyTitle")}
            description={t("sim.emptyBody")}
          />
        </Card>
      )}

      {result && result.groups.length === 0 && (
        <Card>
          <EmptyState
            icon={<ImageOff size={26} />}
            title={t("sim.noneTitle")}
            description={t("sim.noneBody", { images: formatNumber(result.imagesScanned) })}
          />
        </Card>
      )}

      <div className="space-y-5 pb-16">
        {shownGroups.map((group) => (
          <SimilarGroupCard
            key={group.id}
            group={group}
            selected={selected}
            onToggle={(path) =>
              setSelected((current) => {
                const next = new Set(current);
                if (next.has(path)) next.delete(path);
                else next.add(path);
                return next;
              })
            }
            onOpen={(image) => openCompare(group, image)}
            onDelete={openDeletionForImages}
          />
        ))}
        {hiddenGroupCount > 0 && (
          <Card className="flex items-center justify-between gap-3 px-4 py-3">
            <span className="text-[12.5px] text-text-tertiary">
              {t("sim.showingGroups", {
                shown: formatNumber(shownGroups.length),
                total: formatNumber(result?.groups.length ?? 0),
              })}
            </span>
            <Button
              size="sm"
              variant="subtle"
              onClick={() => setVisibleGroups((n) => n + SIMILAR_GROUP_PAGE_SIZE)}
            >
              {t("common.showMore", { n: formatNumber(Math.min(SIMILAR_GROUP_PAGE_SIZE, hiddenGroupCount)) })}
            </Button>
          </Card>
        )}
      </div>

      <DeletionDialog
        open={deletionTarget !== null}
        target={deletionTarget}
        onClose={() => setDeletionTarget(null)}
        onCompleted={(report) => {
          setSelected(new Set());
          if (report.deleted > 0 && result) {
            const removed = new Set(
              report.outcomes.filter((outcome) => outcome.success).map((outcome) => outcome.path),
            );
            setResult({
              ...result,
              groups: result.groups
                .map((group) => ({
                  ...group,
                  images: group.images.filter((image) => !removed.has(image.file.path)),
                }))
                .filter((group) => group.images.length > 1),
            });
          }
        }}
      />

      <CompareModal
        open={compare !== null}
        group={compare?.group ?? null}
        index={compare?.index ?? 0}
        loadFull={loadFull}
        onIndexChange={(index) =>
          setCompare((current) => (current ? { ...current, index } : current))
        }
        onClose={() => setCompare(null)}
      />

    </div>
  );
}

function groupWasted(group: SimilarGroup): number {
  const total = group.images.reduce((sum, image) => sum + image.file.size, 0);
  const keep = Math.max(...group.images.map((image) => image.file.size), 0);
  return Math.max(0, total - keep);
}

function SimilarGroupCard({
  group,
  selected,
  onToggle,
  onOpen,
  onDelete,
}: {
  group: SimilarGroup;
  selected: Set<string>;
  onToggle: (path: string) => void;
  onOpen: (image: SimilarImage) => void;
  onDelete: (paths: string[]) => void;
}) {
  const { t } = useT();
  const selectedInGroup = group.images.filter((image) => selected.has(image.file.path));
  return (
    <Card className="overflow-hidden">
      <div className="flex flex-wrap items-center gap-2.5 border-b border-line-soft px-4 py-3">
        <Badge tone="accent">{t("sim.similarBadge", { n: formatNumber(group.images.length) })}</Badge>
        <Badge tone={group.maxDistance <= 4 ? "success" : "warning"}>
          {t("sim.distanceBits", { n: formatNumber(group.maxDistance) })}
        </Badge>
        <span className="text-[12px] text-text-tertiary">
          {t("sim.reclaimableIfOneKept", { size: formatBytes(groupWasted(group)) })}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            icon={<Recycle size={12} />}
            disabled={selectedInGroup.length === 0}
            onClick={() => onDelete(selectedInGroup.map((image) => image.file.path))}
          >
            {t("sim.deleteSelected")}
          </Button>
        </div>
      </div>
      <div className="grid grid-cols-2 gap-3 p-3 sm:grid-cols-3 lg:grid-cols-4">
        {group.images.map((image) => {
          const isSelected = selected.has(image.file.path);
          return (
            <div
              key={image.file.path}
              className={`group relative overflow-hidden rounded-xl border transition-all ${
                isSelected ? "border-danger/60 ring-2 ring-danger/30" : "border-line-soft hover:border-line"
              }`}
            >
              <button
                className="relative block aspect-[4/3] w-full bg-black/40"
                onClick={() => onOpen(image)}
              >
                {image.thumbnail ? (
                  <img
                    src={image.thumbnail}
                    alt={image.file.name}
                    className="h-full w-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
                    loading="lazy"
                  />
                ) : (
                  <span className="flex h-full items-center justify-center text-text-tertiary">
                    <ImageOff size={20} />
                  </span>
                )}
                <span className="absolute inset-0 flex items-center justify-center bg-black/0 opacity-0 transition-all group-hover:bg-black/35 group-hover:opacity-100">
                  <Maximize2 size={18} className="text-white" />
                </span>
                <span className="absolute left-2 top-2 flex items-center gap-1 rounded-md bg-black/60 px-1.5 py-0.5 text-[10px] text-white backdrop-blur">
                  <Eye size={10} /> {t("sim.matchesCount", { n: formatNumber(group.images.length) })}
                </span>
                <span className="absolute right-2 top-2 rounded-md bg-black/60 px-1.5 py-0.5 font-mono text-[10px] text-white backdrop-blur">
                  Δ{image.distance}
                </span>
              </button>
              <div className="space-y-1 bg-surface-1 p-2.5">
                <div className="flex items-center gap-2">
                  <Checkbox checked={isSelected} onChange={() => onToggle(image.file.path)} />
                  <span className="min-w-0 flex-1 truncate text-[11.5px] text-text-primary" title={image.file.name}>
                    {image.file.name}
                  </span>
                </div>
                <div className="flex items-center justify-between text-[10.5px] text-text-tertiary">
                  <span>{formatBytes(image.file.size)}</span>
                  <span className="font-mono">{image.phash.slice(0, 8)}</span>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}

/**
 * Side-by-side comparison: the group reference on the left, the selected copy on
 * the right, with the perceptual-hash distance and per-file metadata in between.
 * Arrow keys / buttons walk through the copies without leaving the view.
 */
function CompareModal({
  open,
  group,
  index,
  loadFull,
  onIndexChange,
  onClose,
}: {
  open: boolean;
  group: SimilarGroup | null;
  index: number;
  loadFull: (path: string) => Promise<string | null>;
  onIndexChange: (index: number) => void;
  onClose: () => void;
}) {
  const { t } = useT();
  const [left, setLeft] = useState<string | null>(null);
  const [right, setRight] = useState<string | null>(null);

  const reference = group ? referenceOf(group) : null;
  const candidate =
    group && group.images.length > 0
      ? (group.images[Math.min(index, group.images.length - 1)] ?? null)
      : null;

  useEffect(() => {
    if (!open || !reference) {
      setLeft(null);
      return;
    }
    let cancelled = false;
    void loadFull(reference.file.path).then((data) => {
      if (!cancelled) setLeft(data);
    });
    return () => {
      cancelled = true;
    };
  }, [open, reference, loadFull]);

  useEffect(() => {
    if (!open || !candidate) {
      setRight(null);
      return;
    }
    let cancelled = false;
    void loadFull(candidate.file.path).then((data) => {
      if (!cancelled) setRight(data);
    });
    return () => {
      cancelled = true;
    };
  }, [open, candidate, loadFull]);

  useEffect(() => {
    if (!open || !group) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "ArrowRight") {
        onIndexChange((index + 1) % group.images.length);
      } else if (event.key === "ArrowLeft") {
        onIndexChange((index - 1 + group.images.length) % group.images.length);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, group, index, onIndexChange]);

  // `distance` is the Hamming distance from this image to the group reference,
  // computed once in Rust — no need to recompute it in the UI.
  const distance = candidate?.distance ?? 0;
  const sizeDelta =
    reference && candidate ? candidate.file.size - reference.file.size : 0;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("sim.compareTitle")}
      description={
        group
          ? t("sim.compareDescription", {
              index: Math.min(index + 1, group.images.length),
              total: group.images.length,
            })
          : undefined
      }
      width={1040}
      dismissable
      footer={
        <>
          <span className="mr-auto text-[12px] text-text-tertiary">
            {candidate && reference
              ? candidate.file.path === reference.file.path
                ? t("sim.isReference")
                : t("sim.distanceLine", {
                    n: tc("bit", distance),
                    size:
                      sizeDelta === 0
                        ? t("sim.sameSize")
                        : `${sizeDelta > 0 ? "+" : ""}${formatBytes(Math.abs(sizeDelta))} ${t(sizeDelta > 0 ? "sim.larger" : "sim.smaller")}`,
                  })
              : ""}
          </span>
          {group && group.images.length > 1 && (
            <Button
              variant="secondary"
              onClick={() => onIndexChange((index - 1 + group.images.length) % group.images.length)}
            >
              {t("common.previous")}
            </Button>
          )}
          {group && group.images.length > 1 && (
            <Button
              variant="secondary"
              onClick={() => onIndexChange((index + 1) % group.images.length)}
            >
              {t("common.next")}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t("common.close")}
          </Button>
        </>
      }
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <ComparePane
          label={t("sim.reference")}
          image={reference}
          data={left}
          tone="accent"
        />
        <ComparePane
          label={
            candidate && reference && candidate.file.path === reference.file.path
              ? t("sim.sameFile")
              : t("sim.copy")
          }
          image={candidate}
          data={right}
          tone="neutral"
        />
      </div>
    </Modal>
  );
}

function ComparePane({
  label,
  image,
  data,
  tone,
}: {
  label: string;
  image: SimilarImage | null;
  data: string | null;
  tone: "accent" | "neutral";
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-line-soft bg-surface-1">
      <div className="flex items-center justify-between border-b border-line-soft px-3 py-2">
        <span className="text-[11px] font-medium uppercase tracking-wide text-text-tertiary">
          {label}
        </span>
        {image && <Badge tone={tone === "accent" ? "accent" : "neutral"}>{formatBytes(image.file.size)}</Badge>}
      </div>
      <div className="flex h-[42vh] items-center justify-center overflow-hidden bg-black/50">
        {data ? (
          <img src={data} alt={image?.file.name ?? ""} className="max-h-full max-w-full object-contain" />
        ) : image?.thumbnail ? (
          <img src={image.thumbnail} alt={image.file.name} className="max-h-full max-w-full object-contain blur-sm" />
        ) : (
          <ImageOff size={26} className="text-text-tertiary" />
        )}
      </div>
      <div className="space-y-1 px-3 py-2.5">
        <div className="truncate text-[12px] text-text-primary" title={image?.file.path}>
          {image?.file.name ?? "—"}
        </div>
        <div className="truncate text-[10.5px] text-text-tertiary" title={image?.file.path}>
          {image?.file.path ?? ""}
        </div>
        {image && (
          <div className="flex items-center justify-between pt-1 text-[10.5px] text-text-tertiary">
            <span className="font-mono">{image.phash.slice(0, 16)}</span>
            <span>Δ{image.distance}</span>
          </div>
        )}
      </div>
    </div>
  );
}
