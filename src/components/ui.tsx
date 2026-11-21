// Small, dependency-free UI primitives (Fluent-like dark theme).

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import { Check, Loader2, X } from "lucide-react";
import { useT } from "../i18n";

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "subtle";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  icon?: ReactNode;
}

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  primary:
    "bg-accent text-white hover:bg-accent-strong active:scale-[0.985] shadow-[0_8px_24px_-12px_rgba(79,140,255,0.9)] disabled:bg-accent/40",
  secondary:
    "bg-surface-4 text-text-primary hover:bg-[#31353d] active:scale-[0.985] border border-line",
  ghost:
    "bg-transparent text-text-secondary hover:bg-white/[0.06] hover:text-text-primary",
  subtle:
    "bg-white/[0.05] text-text-primary hover:bg-white/[0.09] border border-line-soft",
  danger:
    "bg-danger text-white hover:bg-[#e05a5a] active:scale-[0.985] shadow-[0_8px_24px_-12px_rgba(245,108,108,0.9)]",
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "h-8 px-3 text-xs gap-1.5 rounded-lg",
  md: "h-9.5 px-4 text-[13px] gap-2 rounded-[10px]",
  lg: "h-11 px-5 text-sm gap-2 rounded-xl",
};

export function Button({
  variant = "secondary",
  size = "md",
  loading = false,
  icon,
  className,
  children,
  disabled,
  ...rest
}: ButtonProps) {
  return (
    <button
      {...rest}
      disabled={disabled || loading}
      className={cx(
        "inline-flex items-center justify-center font-medium transition-all duration-150 select-none",
        "disabled:opacity-50 disabled:pointer-events-none focus-ring",
        VARIANT_CLASSES[variant],
        SIZE_CLASSES[size],
        className,
      )}
    >
      {loading ? <Loader2 size={15} className="animate-spin" /> : icon}
      {children}
    </button>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  active?: boolean;
  danger?: boolean;
}

export function IconButton({
  label,
  active,
  danger,
  className,
  children,
  ...rest
}: IconButtonProps) {
  return (
    <button
      {...rest}
      aria-label={label}
      title={label}
      className={cx(
        "inline-flex h-8 w-8 items-center justify-center rounded-lg transition-colors focus-ring",
        active
          ? "bg-white/10 text-text-primary"
          : "text-text-secondary hover:bg-white/[0.07] hover:text-text-primary",
        danger && "hover:bg-danger/15 hover:text-danger",
        className,
      )}
    >
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Cards / layout
// ---------------------------------------------------------------------------

export function Card({
  className,
  children,
  onClick,
}: {
  className?: string;
  children: ReactNode;
  onClick?: () => void;
}) {
  return (
    <div onClick={onClick} className={cx("card", className)}>
      {children}
    </div>
  );
}

export function SectionTitle({
  title,
  subtitle,
  action,
}: {
  title: string;
  subtitle?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 mb-4">
      <div>
        <h2 className="text-[15px] font-semibold text-text-primary">{title}</h2>
        {subtitle && (
          <p className="text-xs text-text-tertiary mt-0.5">{subtitle}</p>
        )}
      </div>
      {action}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Toggle / Checkbox
// ---------------------------------------------------------------------------

export function Toggle({
  checked,
  onChange,
  label,
  description,
  disabled,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
  disabled?: boolean;
}) {
  return (
    <label
      className={cx(
        "flex items-center justify-between gap-4 py-2.5",
        disabled ? "opacity-50" : "cursor-pointer",
      )}
    >
      <span>
        <span className="block text-[13px] text-text-primary">{label}</span>
        {description && (
          <span className="block text-xs text-text-tertiary mt-0.5">
            {description}
          </span>
        )}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cx(
          "relative h-6 w-11 shrink-0 rounded-full transition-colors duration-200 focus-ring",
          checked ? "bg-accent" : "bg-[#3a3e46]",
        )}
      >
        <span
          className={cx(
            "absolute top-0.5 left-0.5 h-5 w-5 rounded-full bg-white transition-transform duration-200",
            checked ? "translate-x-5" : "translate-x-0",
          )}
        />
      </button>
    </label>
  );
}

export function Checkbox({
  checked,
  indeterminate = false,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  indeterminate?: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
}) {
  return (
    <label
      className={cx(
        "inline-flex items-center gap-2 select-none",
        disabled ? "opacity-40" : "cursor-pointer",
      )}
    >
      <span
        role="checkbox"
        aria-checked={indeterminate ? "mixed" : checked}
        tabIndex={0}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (!disabled) onChange(!checked);
        }}
        onKeyDown={(event) => {
          if (event.key === " " || event.key === "Enter") {
            event.preventDefault();
            if (!disabled) onChange(!checked);
          }
        }}
        className={cx(
          "flex h-[18px] w-[18px] items-center justify-center rounded-[5px] border transition-all duration-150",
          checked || indeterminate
            ? "border-accent bg-accent text-white"
            : "border-[#4a4f58] bg-transparent hover:border-[#6a707b]",
        )}
      >
        {indeterminate ? (
          <span className="h-[2px] w-2.5 rounded-full bg-white" />
        ) : checked ? (
          <Check size={13} strokeWidth={3.2} />
        ) : null}
      </span>
      {label && <span className="text-[13px] text-text-secondary">{label}</span>}
    </label>
  );
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

export function ProgressBar({
  value,
  indeterminate,
  className,
  tone = "accent",
}: {
  value: number;
  indeterminate?: boolean;
  className?: string;
  tone?: "accent" | "success" | "danger";
}) {
  const colors = {
    accent: "bg-accent",
    success: "bg-success",
    danger: "bg-danger",
  } as const;
  return (
    <div
      className={cx(
        "relative h-1.5 w-full overflow-hidden rounded-full bg-white/[0.08]",
        indeterminate && "progress-indeterminate",
        className,
      )}
    >
      {!indeterminate && (
        <div
          className={cx("h-full rounded-full transition-[width] duration-300 ease-out", colors[tone])}
          style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
        />
      )}
    </div>
  );
}

export function Spinner({ size = 18 }: { size?: number }) {
  return <Loader2 size={size} className="animate-spin text-accent" />;
}

// ---------------------------------------------------------------------------
// Badge / chip
// ---------------------------------------------------------------------------

export function Badge({
  children,
  tone = "neutral",
  className,
  title,
}: {
  children: ReactNode;
  tone?: "neutral" | "accent" | "success" | "warning" | "danger";
  className?: string;
  /** Native tooltip — used to explain where a badge comes from. */
  title?: string;
}) {
  const tones = {
    neutral: "bg-white/[0.07] text-text-secondary",
    accent: "bg-accent-soft text-accent",
    success: "bg-success/15 text-success",
    warning: "bg-warning/15 text-warning",
    danger: "bg-danger/15 text-danger",
  } as const;
  return (
    <span
      title={title}
      className={cx(
        "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-medium",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

interface TextInputProps extends InputHTMLAttributes<HTMLInputElement> {
  icon?: ReactNode;
  suffix?: ReactNode;
}

export function TextInput({ icon, suffix, className, ...rest }: TextInputProps) {
  return (
    <div
      className={cx(
        "flex h-9 items-center gap-2 rounded-[10px] border border-line bg-surface-1 px-3",
        "focus-within:border-accent/60 focus-within:ring-2 focus-within:ring-accent/20 transition-all",
        className,
      )}
    >
      {icon && <span className="text-text-tertiary shrink-0">{icon}</span>}
      <input
        {...rest}
        className="h-full w-full bg-transparent text-[13px] text-text-primary placeholder:text-text-tertiary outline-none"
      />
      {suffix}
    </div>
  );
}

export function NumberInput({
  value,
  onChange,
  min = 0,
  max,
  step = 1,
  className,
  placeholder,
}: {
  value: number | null;
  onChange: (value: number | null) => void;
  min?: number;
  max?: number;
  step?: number;
  className?: string;
  placeholder?: string;
}) {
  return (
    <input
      type="number"
      value={value ?? ""}
      min={min}
      max={max}
      step={step}
      placeholder={placeholder}
      onChange={(event) => {
        const raw = event.target.value;
        if (raw === "") {
          onChange(null);
          return;
        }
        const parsed = Number(raw);
        if (Number.isFinite(parsed)) {
          onChange(Math.min(max ?? Number.MAX_SAFE_INTEGER, Math.max(min, parsed)));
        }
      }}
      className={cx(
        "h-9 w-full rounded-[10px] border border-line bg-surface-1 px-3 text-[13px] text-text-primary",
        "outline-none focus:border-accent/60 focus:ring-2 focus:ring-accent/20 transition-all",
        className,
      )}
    />
  );
}

// ---------------------------------------------------------------------------
// Modal
// ---------------------------------------------------------------------------

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 560,
  dismissable = true,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  dismissable?: boolean;
}) {
  const { t } = useT();
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && dismissable) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose, dismissable]);

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 backdrop-blur-[3px] animate-fade-in"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && dismissable) onClose();
      }}
    >
      <div
        className="animate-slide-up rounded-[18px] border border-line bg-surface-2 shadow-2xl"
        style={{ width, maxWidth: "calc(100vw - 48px)", maxHeight: "calc(100vh - 80px)" }}
      >
        <div className="flex items-start justify-between gap-4 border-b border-line-soft px-6 pt-5 pb-4">
          <div>
            <h3 className="text-base font-semibold text-text-primary">{title}</h3>
            {description && (
              <p className="mt-1 text-xs text-text-tertiary">{description}</p>
            )}
          </div>
          {dismissable && (
            <IconButton label={t("common.close")} onClick={onClose}>
              <X size={16} />
            </IconButton>
          )}
        </div>
        <div className="max-h-[55vh] overflow-y-auto px-6 py-4">{children}</div>
        {footer && (
          <div className="flex items-center justify-end gap-2 border-t border-line-soft px-6 py-4">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Context menu
// ---------------------------------------------------------------------------

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onSelect: () => void;
  danger?: boolean;
  disabled?: boolean;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose]);

  const adjustedX = Math.min(x, window.innerWidth - 230);
  const adjustedY = Math.min(y, window.innerHeight - items.length * 36 - 16);

  return (
    <div
      ref={ref}
      className="fixed z-[60] min-w-[210px] overflow-hidden rounded-xl border border-line bg-[#1d1f24] p-1 shadow-2xl animate-fade-in"
      style={{ left: adjustedX, top: adjustedY }}
      onContextMenu={(event) => event.preventDefault()}
    >
      {items.map((item) => (
        <button
          key={item.label}
          disabled={item.disabled}
          onClick={() => {
            item.onSelect();
            onClose();
          }}
          className={cx(
            "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[13px] transition-colors",
            item.disabled
              ? "cursor-not-allowed text-text-tertiary"
              : "text-text-primary hover:bg-white/[0.08]",
            item.danger && !item.disabled && "text-danger hover:bg-danger/12",
          )}
        >
          <span className={cx("shrink-0", item.danger ? "text-danger" : "text-text-secondary")}>
            {item.icon}
          </span>
          {item.label}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
      <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-white/[0.05] text-text-tertiary">
        {icon}
      </div>
      <h3 className="text-[15px] font-semibold text-text-primary">{title}</h3>
      {description && (
        <p className="mt-1.5 max-w-md text-[13px] leading-relaxed text-text-tertiary">
          {description}
        </p>
      )}
      {action && <div className="mt-5">{action}</div>}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

export interface ToastMessage {
  id: number;
  tone: "info" | "success" | "error";
  title: string;
  description?: string;
}

interface ToastContextValue {
  push: (tone: ToastMessage["tone"], title: string, description?: string) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function useToast(): ToastContextValue {
  const context = useContext(ToastContext);
  if (!context) {
    // Fallback keeps components usable outside the provider (e.g. tests).
    return { push: () => undefined };
  }
  return context;
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastMessage[]>([]);
  const counter = useRef(0);

  const push: ToastContextValue["push"] = (tone, title, description) => {
    counter.current += 1;
    const id = counter.current;
    setToasts((current) => [...current.slice(-4), { id, tone, title, description }]);
    window.setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id));
    }, tone === "error" ? 7000 : 4200);
  };

  return (
    <ToastContext.Provider value={{ push }}>
      {children}
      <div className="pointer-events-none fixed bottom-5 right-5 z-[70] flex w-[340px] flex-col gap-2">
        {toasts.map((toast) => (
          <div
            key={toast.id}
            className={cx(
              "pointer-events-auto animate-slide-up rounded-xl border px-4 py-3 shadow-2xl backdrop-blur-xl",
              toast.tone === "error" && "border-danger/35 bg-[#2a1a1c]/95",
              toast.tone === "success" && "border-success/35 bg-[#14251d]/95",
              toast.tone === "info" && "border-line bg-[#1b1d22]/95",
            )}
          >
            <div className="flex items-start gap-2.5">
              <span
                className={cx(
                  "mt-1 h-2 w-2 shrink-0 rounded-full",
                  toast.tone === "error" && "bg-danger",
                  toast.tone === "success" && "bg-success",
                  toast.tone === "info" && "bg-accent",
                )}
              />
              <div className="min-w-0">
                <div className="text-[13px] font-medium text-text-primary">
                  {toast.title}
                </div>
                {toast.description && (
                  <div className="mt-0.5 break-words text-xs leading-relaxed text-text-secondary">
                    {toast.description}
                  </div>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

export function KeyHint({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-line bg-surface-3 px-1.5 py-0.5 font-mono text-[10px] text-text-tertiary">
      {children}
    </kbd>
  );
}

export function Divider({ className }: { className?: string }) {
  return <div className={cx("h-px bg-line-soft", className)} />;
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

export function Tabs({
  value,
  onChange,
  tabs,
}: {
  value: string;
  onChange: (value: string) => void;
  tabs: { id: string; label: string; icon?: ReactNode }[];
}) {
  return (
    <div className="inline-flex items-center gap-1 rounded-xl border border-line-soft bg-surface-1 p-1">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          onClick={() => onChange(tab.id)}
          className={cx(
            "flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12.5px] transition-colors",
            tab.id === value
              ? "bg-white/[0.09] font-medium text-text-primary"
              : "text-text-secondary hover:text-text-primary",
          )}
        >
          {tab.icon}
          {tab.label}
        </button>
      ))}
    </div>
  );
}
