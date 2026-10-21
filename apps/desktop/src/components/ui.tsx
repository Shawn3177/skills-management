import { useEffect, useRef, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import claudeCodeLogo from "../assets/tools/claude-code.svg";
import codexLogo from "../assets/tools/codex.svg";
import githubMark from "../assets/tools/github.svg";
import codebuddyLogo from "../assets/tools/codebuddy.svg";
import geminiLogo from "../assets/tools/gemini-cli.svg";
import grokLogo from "../assets/tools/grok-build.svg";
import hermesLogo from "../assets/tools/hermes.svg";
import openclawLogo from "../assets/tools/openclaw.svg";
import opencodeLogo from "../assets/tools/opencode.svg";
import piLogo from "../assets/tools/pi.svg";
import qoderLogo from "../assets/tools/qoder.svg";
import qwenLogo from "../assets/tools/qwen-code.svg";
import workbuddyLogo from "../assets/tools/workbuddy.png";
import { monogram, tileColors } from "../lib/skills";

const toolLogos: Record<string, string> = {
  "claude-code": claudeCodeLogo,
  codex: codexLogo,
  "gemini-cli": geminiLogo,
  "grok-build": grokLogo,
  opencode: opencodeLogo,
  openclaw: openclawLogo,
  hermes: hermesLogo,
  pi: piLogo,
  "qwen-code": qwenLogo,
  qoder: qoderLogo,
  codebuddy: codebuddyLogo,
  workbuddy: workbuddyLogo,
};

/** A tool's own logo; greyed out where the tool is off or not supported yet. */
export function ToolLogo({ id, dim = false }: { id: string; dim?: boolean }) {
  const logo = toolLogos[id];
  if (!logo) {
    return null;
  }
  return <img className={`tool-logo ${dim ? "dim" : ""}`} src={logo} alt="" width={18} height={18} aria-hidden="true" />;
}

/** Opens a skill's (or a series') GitHub page. Icon only unless `withLabel`. */
export function GithubButton({
  t,
  name,
  url,
  onOpenUrl,
  withLabel = false,
}: {
  t: (key: "series.openGithubLabel", params: { repo: string }) => string;
  name: string;
  url: string;
  onOpenUrl: (url: string) => void;
  withLabel?: boolean;
}) {
  const label = t("series.openGithubLabel", { repo: name });
  return (
    <button
      type="button"
      className={withLabel ? "button small github-button" : "icon-button github-button"}
      aria-label={label}
      title={url.replace(/^https?:\/\//, "")}
      onClick={() => onOpenUrl(url)}
    >
      <img src={githubMark} alt="" width={15} height={15} aria-hidden="true" />
      {withLabel ? "GitHub" : null}
    </button>
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
