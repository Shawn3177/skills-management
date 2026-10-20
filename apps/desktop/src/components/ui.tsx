import { useEffect, useRef, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { monogram, tileColors } from "../lib/skills";

/** The small rounded letter tile used for tools (Claude Code, Codex, …). */
export function ToolMark({ mono, color, round = false }: { mono: string; color: string; round?: boolean }) {
  return (
    <span className={`tool-mark ${round ? "round" : ""}`} style={{ background: color }} aria-hidden="true">
      {mono}
    </span>
  );
}

/** A skill's icon: a soft tile with up to two characters of its name. */
export function SkillTile({ name, muted = false }: { name: string; muted?: boolean }) {
  const colors = muted ? { background: "#ffffff", color: "#6b7280" } : tileColors(name);
  return (
    <span className="skill-tile" style={colors} aria-hidden="true">
      {monogram(name)}
    </span>
  );
}

/** Sub-page header (CC Switch style): a square back button and a title. */
export function PageHeader({ title, backLabel, onBack, children }: {
  title: string;
  backLabel: string;
  onBack: () => void;
  children?: ReactNode;
}) {
  const headingRef = useRef<HTMLHeadingElement>(null);

  // Move focus to the new page's title so keyboard and screen-reader users
  // land on the page they just opened.
  useEffect(() => {
    headingRef.current?.focus();
  }, [title]);

  return (
    <header className="page-header" data-tauri-drag-region>
      <button type="button" className="back-button" aria-label={backLabel} title={backLabel} onClick={onBack}>
        <ArrowLeft size={16} strokeWidth={2} aria-hidden="true" />
      </button>
      <h1 ref={headingRef} tabIndex={-1} data-tauri-drag-region>
        {title}
      </h1>
      {children}
      <span className="drag-fill" data-tauri-drag-region />
    </header>
  );
}

export type Notice = {
  kind: "success" | "error" | "warning";
  text: string;
  /** Optional follow-ups the user can choose (the first is the recommended one). */
  actions?: Array<{ label: string; run: () => void }>;
} | null;

export function NoticeBar({ notice }: { notice: Notice }) {
  if (!notice) {
    return null;
  }
  return (
    <div className={`notice ${notice.kind}`} role={notice.kind === "success" ? "status" : "alert"}>
      <span>{notice.text}</span>
      {notice.actions?.map((action, index) => (
        <button
          type="button"
          key={action.label}
          className={`button small ${index === 0 && notice.actions && notice.actions.length > 1 ? "primary" : ""}`}
          onClick={action.run}
        >
          {action.label}
        </button>
      ))}
    </div>
  );
}

/** A pressable chip (CC Switch's preset chips). */
export function Chip({ pressed, disabled, onClick, children, label }: {
  pressed: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
  label?: string;
}) {
  return (
    <button
      type="button"
      className={`chip ${pressed ? "on" : ""}`}
      aria-pressed={pressed}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
