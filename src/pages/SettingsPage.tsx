// Settings: engine, safety and maintenance options persisted in SQLite.

import { useCallback, useEffect, useState } from "react";
import {
  Check,
  Cpu,
  Database,
  Filter,
  FolderX,
  Plus,
  Languages,
  RotateCcw,
  Save,
  ShieldCheck,
  Trash2,
  Zap,
} from "lucide-react";
import type { AppSettings } from "../types";
import { api, errorMessage, isTauri } from "../services/api";
import { MAX_SIMILAR_THRESHOLD, formatNumber } from "../services/format";
import { localizeBackend, setLang, useT } from "../i18n";
import type { Lang } from "../i18n";
import {
  Badge,
  Button,
  Card,
  SectionTitle,
  Spinner,
  TextInput,
  Toggle,
  useToast,
} from "../components/ui";

export function SettingsPage() {
  const toast = useToast();
  const { t, lang } = useT();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [newExclusion, setNewExclusion] = useState("");
  const [clearing, setClearing] = useState(false);

  const load = useCallback(async () => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const value = await api.getSettings();
      setSettings(value);
      setDraft(value);
    } catch (error) {
      toast.push("error", t("settings.loadFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    void load();
  }, [load]);

  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
  };

  const save = async () => {
    if (!draft) return;
    setSaving(true);
    try {
      const saved = await api.saveSettings(draft);
      setSettings(saved);
      setDraft(saved);
      toast.push("success", t("settings.saveOk"));
    } catch (error) {
      toast.push("error", t("settings.saveFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setSaving(false);
    }
  };

  const clearCache = async () => {
    setClearing(true);
    try {
      const removed = await api.clearHashCache();
      toast.push(
        "success",
        t("settings.cacheCleared"),
        t("settings.cacheClearedBody", { n: formatNumber(removed) }),
      );
    } catch (error) {
      toast.push("error", t("settings.cacheClearFailed"), localizeBackend(errorMessage(error)));
    } finally {
      setClearing(false);
    }
  };

  const dirty = JSON.stringify(settings) !== JSON.stringify(draft);

  if (loading || !draft) {
    return (
      <div className="flex items-center gap-3 py-16 text-[13px] text-text-secondary">
        <Spinner /> {t("settings.loading")}
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-[900px] space-y-5 pb-20">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-[26px] font-semibold tracking-tight text-text-primary">{t("settings.title")}</h1>
          <p className="mt-1 text-[13px] text-text-secondary">
            {t("settings.subtitle")}
          </p>
        </div>
        <Button
          variant="primary"
          icon={dirty ? <Save size={15} /> : <Check size={15} />}
          disabled={!dirty}
          loading={saving}
          onClick={() => void save()}
        >
          {dirty ? t("settings.save") : t("settings.saved")}
        </Button>
      </div>

      <Card className="p-5">
        <SectionTitle
          title={t("settings.language")}
          subtitle={t("settings.languageSubtitle")}
          action={
            <Badge tone="accent">
              <Languages size={11} /> {lang.toUpperCase()}
            </Badge>
          }
        />
        <div className="flex flex-wrap gap-2">
          {(["en", "ru"] as Lang[]).map((value) => (
            <Button
              key={value}
              size="sm"
              variant={lang === value ? "primary" : "subtle"}
              onClick={() => setLang(value)}
            >
              {value === "en" ? t("settings.languageEn") : t("settings.languageRu")}
            </Button>
          ))}
        </div>
      </Card>

      <Card className="p-5">
        <SectionTitle
          title={t("settings.performance")}
          subtitle={t("settings.performanceSubtitle")}
          action={<Badge tone="accent"><Cpu size={11} /> {t("settings.engineBadge")}</Badge>}
        />
        <div className="mb-1.5 flex items-center justify-between text-[12.5px] text-text-secondary">
          <span>{t("settings.workers")}</span>
          <span className="font-mono text-text-primary">
            {draft.hashConcurrency === 0
              ? t("settings.workerAuto", { n: navigator.hardwareConcurrency || 4 })
              : draft.hashConcurrency}
          </span>
        </div>
        <input
          type="range"
          min={0}
          max={16}
          value={draft.hashConcurrency}
          onChange={(event) => update("hashConcurrency", Number(event.target.value))}
          className="w-full accent-[#4f8cff]"
        />
        <div className="mt-1 flex justify-between text-[10.5px] text-text-tertiary">
          <span>{t("settings.workersAuto")}</span>
          <span>{t("settings.workersMax")}</span>
        </div>
        <p className="mt-3 text-[11.5px] leading-relaxed text-text-tertiary">
          {t("settings.performanceBody")}
        </p>
      </Card>

      <Card className="p-5">
        <SectionTitle
          title={t("settings.safety")}
          subtitle={t("settings.safetySubtitle")}
          action={<Badge tone="success"><ShieldCheck size={11} /> {t("settings.protectedBadge")}</Badge>}
        />
        <div className="divide-y divide-line-soft/60">
          <Toggle
            checked={draft.confirmDeletion}
            onChange={(value) => update("confirmDeletion", value)}
            label={t("settings.confirmDeletion")}
            description={t("settings.confirmDeletionBody")}
          />
          <Toggle
            checked={draft.verifyHashBeforeDelete}
            onChange={(value) => update("verifyHashBeforeDelete", value)}
            label={t("settings.verifyHash")}
            description={t("settings.verifyHashBody")}
          />
          <Toggle
            checked={draft.useRecycleBin}
            onChange={(value) => update("useRecycleBin", value)}
            label={t("settings.useRecycleBin")}
            description={t("settings.useRecycleBinBody")}
          />
        </div>
        <div className="mt-3 rounded-xl border border-line-soft bg-surface-1 px-3.5 py-3 text-[11.5px] leading-relaxed text-text-tertiary">
          <span className="font-medium text-text-secondary">{t("settings.lastCopyGuard")}</span>
          {t("settings.lastCopyGuardBody")}
        </div>
      </Card>

      <Card className="p-5">
        <SectionTitle title={t("settings.similar")} subtitle={t("settings.similarSubtitle")} />
        <div className="mb-1.5 flex items-center justify-between text-[12.5px] text-text-secondary">
          <span>{t("settings.hammingThreshold")}</span>
          <span className="font-mono text-text-primary">
            {t("sim.bits", { n: draft.similarPhotosThreshold })}
          </span>
        </div>
        <input
          type="range"
          min={0}
          max={MAX_SIMILAR_THRESHOLD}
          value={draft.similarPhotosThreshold}
          onChange={(event) => update("similarPhotosThreshold", Number(event.target.value))}
          className="w-full accent-[#4f8cff]"
        />
        <p className="mt-2 text-[11.5px] text-text-tertiary">
          {t("settings.hammingBody")}
        </p>
      </Card>

      <Card className="p-5">
        <SectionTitle
          title={t("settings.excluded")}
          subtitle={t("settings.excludedSubtitle")}
          action={<Badge><FolderX size={11} /> {draft.excludedDirNames.length}</Badge>}
        />
        <div className="flex flex-wrap gap-1.5">
          {draft.excludedDirNames.map((name) => (
            <span
              key={name}
              className="flex items-center gap-1.5 rounded-lg border border-line-soft bg-surface-1 px-2.5 py-1 font-mono text-[11.5px] text-text-secondary"
            >
              {name}
              <button
                className="text-text-tertiary hover:text-danger"
                aria-label={t("settings.removeExclusion", { name })}
                onClick={() =>
                  update(
                    "excludedDirNames",
                    draft.excludedDirNames.filter((entry) => entry !== name),
                  )
                }
              >
                ×
              </button>
            </span>
          ))}
        </div>
        <div className="mt-3 flex gap-2">
          <TextInput
            className="flex-1"
            placeholder={t("settings.excludedPlaceholder")}
            value={newExclusion}
            onChange={(event) => setNewExclusion(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && newExclusion.trim()) {
                update("excludedDirNames", [...draft.excludedDirNames, newExclusion.trim()]);
                setNewExclusion("");
              }
            }}
          />
          <Button
            variant="subtle"
            icon={<Plus size={14} />}
            onClick={() => {
              if (newExclusion.trim()) {
                update("excludedDirNames", [...draft.excludedDirNames, newExclusion.trim()]);
                setNewExclusion("");
              }
            }}
          >
            {t("common.add")}
          </Button>
        </div>
      </Card>

      <Card className="p-5">
        <SectionTitle title={t("settings.storage")} subtitle={t("settings.storageSubtitle")} />
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div className="rounded-xl border border-line-soft bg-surface-1 p-3.5">
            <div className="flex items-center gap-2 text-[12px] text-text-secondary">
              <Filter size={13} /> {t("settings.defaultMinSize")}
            </div>
            <div className="mt-2 flex items-center gap-2">
              <input
                type="number"
                min={0}
                max={1024}
                value={Math.round(draft.minFileSize / 1024) || 0}
                onChange={(event) => {
                  const kilobytes = Math.max(0, Math.min(1024, Number(event.target.value) || 0));
                  update("minFileSize", kilobytes * 1024);
                }}
                className="h-8 w-24 rounded-lg border border-line bg-surface-2 px-2 text-[12.5px] text-text-primary outline-none focus:border-accent/60"
              />
              <span className="text-[11px] text-text-tertiary">
                {t("settings.minSizeUnit")}
              </span>
            </div>
          </div>
          <div className="rounded-xl border border-line-soft bg-surface-1 p-3.5">
            <div className="flex items-center gap-2 text-[12px] text-text-secondary">
              <Database size={13} /> {t("settings.historyLimit")}
            </div>
            <div className="mt-2 flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={500}
                value={draft.historyLimit}
                onChange={(event) => update("historyLimit", Math.max(1, Math.min(500, Number(event.target.value) || 1)))}
                className="h-8 w-24 rounded-lg border border-line bg-surface-2 px-2 text-[12.5px] text-text-primary outline-none focus:border-accent/60"
              />
              <span className="text-[11px] text-text-tertiary">{t("settings.historyLimitUnit")}</span>
            </div>
          </div>
          <div className="rounded-xl border border-line-soft bg-surface-1 p-3.5">
            <div className="flex items-center gap-2 text-[12px] text-text-secondary">
              <Zap size={13} /> {t("settings.hashCache")}
            </div>
            <div className="mt-2 flex items-center gap-2">
              <Button size="sm" variant="subtle" icon={<Trash2 size={12} />} loading={clearing} onClick={() => void clearCache()}>
                {t("settings.clearCache")}
              </Button>
              <span className="text-[11px] text-text-tertiary">{t("settings.cacheExpiry")}</span>
            </div>
          </div>
        </div>
      </Card>

      <div className="flex items-center justify-between">
        <Button variant="ghost" icon={<RotateCcw size={14} />} onClick={() => setDraft(settings)} disabled={!dirty}>
          {t("settings.discard")}
        </Button>
        <span className="text-[11px] text-text-tertiary">{t("settings.version")}</span>
      </div>
    </div>
  );
}
