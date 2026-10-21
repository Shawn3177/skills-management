import { Check, ChevronDown } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import appIcon from "../assets/app-icon-64.png";
import type { TFunction } from "../i18n/useLocale";
import { allTools, enabledCount, isToolInstalled, tools, type SkillRecord, type Tool } from "../lib/skills";
import type { Tab } from "./MainView";
import { ToolLogo } from "./ui";

type Props = {
  t: TFunction;
  tab: Tab;
  library: SkillRecord[];
  onChange: (tab: Tab) => void;
};

/**
 * One button that shows the current view ("All" or one tool); clicking it
 * opens a list of every option. Keeps the top bar to a single control however
 * many tools are supported.
 */
export function ToolSwitcher({ t, tab, library, onChange }: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const current = allTools.find((tool) => tool.id === tab);
  const currentName = current ? current.name : t("tabs.all");
  const currentCount = current ? enabledCount(library, current.id) : library.length;
  // Tools in use first; the rest (not installed, or hidden in Settings) below.
  const main = tools;
  const others = allTools.filter((tool) => !tools.includes(tool));

  useEffect(() => {
    if (!open) {
      return;
    }
    const closeOnOutside = (event: MouseEvent) => {
      if (root.current && event.target instanceof Node && !root.current.contains(event.target)) {
        setOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", closeOnOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeOnOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [open]);

  function choose(next: Tab) {
    onChange(next);
    setOpen(false);
  }

  function option(id: Tab, name: string, count: number | null, logo: ReactNode, dim = false) {
    const selected = tab === id;
    return (
      <button
        type="button"
        role="option"
        key={id}
        aria-selected={selected}
        aria-label={count === null ? `${name} ${t("settings.notInstalled")}` : `${name} ${count}`}
        className={`switcher-option ${selected ? "selected" : ""} ${dim ? "dim" : ""}`}
        onClick={() => choose(id)}
      >
        {logo}
        <span className="switcher-name">{name}</span>
        {count === null ? <span className="muted small">{t("settings.notInstalled")}</span> : <span className="tab-count">{count}</span>}
        <Check className="switcher-check" size={14} strokeWidth={2.4} aria-hidden="true" />
      </button>
    );
  }

  const toolOption = (tool: Tool) =>
    option(
      tool.id,
      tool.name,
      isToolInstalled(tool.id) || tools.includes(tool) ? enabledCount(library, tool.id) : null,
      <ToolLogo id={tool.id} dim={!isToolInstalled(tool.id) && !tools.includes(tool)} />,
      !isToolInstalled(tool.id) && !tools.includes(tool),
    );

  return (
    <div className={`tool-switcher ${open ? "open" : ""}`} ref={root}>
      <button
        type="button"
        className="switcher-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={t("switcher.label", { tool: currentName })}
        onClick={() => setOpen((value) => !value)}
      >
        {/* Re-keyed on change so the new selection fades in. */}
        <span className="switcher-current" key={tab}>
          {current ? (
            <ToolLogo id={current.id} />
          ) : (
            <img className="tool-logo" src={appIcon} alt="" width={18} height={18} aria-hidden="true" />
          )}
          <span>{currentName}</span>
          <span className="tab-count">{currentCount}</span>
        </span>
        <ChevronDown className="switcher-chevron" size={15} strokeWidth={2.2} aria-hidden="true" />
      </button>

      {open ? (
        <div className="switcher-menu" role="listbox" aria-label={t("tabs.label")}>
          {option(
            "all",
            t("tabs.all"),
            library.length,
            <img className="tool-logo" src={appIcon} alt="" width={18} height={18} aria-hidden="true" />,
          )}
          <span className="switcher-divider" aria-hidden="true" />
          {main.map(toolOption)}
          {others.length > 0 ? (
            <>
              <span className="switcher-group">{t("switcher.others")}</span>
              {others.map(toolOption)}
            </>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
