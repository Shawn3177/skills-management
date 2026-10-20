import { Check, ChevronRight, ExternalLink, FolderGit2, FolderOpen, Link2, Pencil, Search, Unlink } from "lucide-react";
import { useState, type FormEvent, type ReactNode } from "react";
import type { TFunction } from "../i18n/useLocale";
import type { SkillUpdateStatus } from "../lib/api";
import {
  enabledCount,
  githubSource,
  groupBySeries,
  hasConflictIn,
  isEnabledIn,
  sourceLine,
  tildePath,
  tools,
  type SkillRecord,
  type SkillSeries,
  type ToolId,
} from "../lib/skills";
import { SkillTile, ToolMark } from "./ui";

export type Tab = "all" | ToolId;
export type ScanState = "scanning" | "ready" | "preview" | "error";

type Props = {
  t: TFunction;
  tab: Tab;
  library: SkillRecord[];
  visibleLibrary: SkillRecord[];
  locals: SkillRecord[];
  query: string;
  onQueryChange: (query: string) => void;
  scanState: ScanState;
  updates: Map<string, SkillUpdateStatus>;
  busyKey: string | null;
  onToggle: (skill: SkillRecord, toolId: ToolId, enable: boolean) => void;
  onOpen: (skill: SkillRecord) => void;
  onOpenFolder: (path: string) => void;
  onOpenUrl: (url: string) => void;
  onAddToLibrary: (skill: SkillRecord) => void;
  /** A tool already has a different same-named folder: ask before replacing. */
  onConflict: (skill: SkillRecord, toolId: ToolId) => void;
  /** Group installed skills under a GitHub repository; resolves true on success. */
  onLinkSeries: (url: string) => Promise<boolean>;
  onUnlinkSeries: (repository: string) => void;
  /** The user's home folder, to show paths as `~/…`. */
  home: string | null;
};

export function MainView(props: Props) {
  const { t, tab, library, visibleLibrary, locals, query, onQueryChange, scanState } = props;
  const currentTool = tools.find((tool) => tool.id === tab);
  const summary = currentTool
    ? t("summary.tool", { total: library.length, tool: currentTool.name, count: enabledCount(library, currentTool.id) })
    : t("summary.all", {
        total: library.length,
        tools: tools.length,
        links: tools.reduce((sum, tool) => sum + enabledCount(library, tool.id), 0),
      });
  // Rescans after every change and on window focus stay quiet; the note only
  // shows while the very first scan is still running.
  const scanNote =
    scanState === "scanning" && library.length === 0
      ? t("status.scanning")
      : scanState === "error"
        ? t("status.error")
        : scanState === "preview"
          ? t("status.preview")
          : "";
  const toolLocals = currentTool ? locals.filter((skill) => skill.source === currentTool.sourceLabel) : locals;
  const [expanded, setExpanded] = useState<Set<string>>(readExpanded);
  const [linking, setLinking] = useState(false);
  // While searching, every series with a match is shown open.
  const searching = query.trim() !== "";
  const libraryGroups = groupBySeries(visibleLibrary);
  const localGroups = groupBySeries(toolLocals);

  function toggleSeries(key: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (!next.delete(key)) {
        next.add(key);
      }
      writeExpanded(next);
      return next;
    });
  }

  function renderSeries(series: SkillSeries, scope: string, summaryText: string, card: (skill: SkillRecord) => ReactNode) {
    const key = `${scope}:${series.key}`;
    return (
      <SeriesGroup
        key={key}
        id={key}
        t={t}
        series={series}
        summary={summaryText}
        open={searching || expanded.has(key)}
        canToggle={!searching}
        busy={props.busyKey !== null}
        onToggle={() => toggleSeries(key)}
        onOpenUrl={props.onOpenUrl}
        onUnlink={props.onUnlinkSeries}
      >
        {series.skills.map(card)}
      </SeriesGroup>
    );
  }

  const libraryCard = (skill: SkillRecord) => <SkillCard key={skill.id} {...props} skill={skill} />;
  const localCard = (skill: SkillRecord) => <LocalCard key={skill.id} {...props} skill={skill} />;

  return (
    <div className="main-view">
      <div className="toolbar-row">
        <span className="summary">{summary}</span>
        <button
          type="button"
          className={`button ghost small link-toggle ${linking ? "active" : ""}`}
          aria-expanded={linking}
          onClick={() => setLinking((value) => !value)}
        >
          <Link2 size={14} strokeWidth={2} aria-hidden="true" />
          {t("series.linkButton")}
        </button>
        <label className="search-field">
          <Search size={14} strokeWidth={2} aria-hidden="true" />
          <span className="sr-only">{t("search.label")}</span>
          <input
            type="search"
            aria-label={t("search.label")}
            placeholder={t("search.placeholder")}
            value={query}
            onChange={(event) => onQueryChange(event.currentTarget.value)}
          />
        </label>
      </div>

      {linking ? (
        <LinkSeriesPanel
          t={t}
          busy={props.busyKey !== null}
          onCancel={() => setLinking(false)}
          onSubmit={async (url) => {
            if (await props.onLinkSeries(url)) {
              setLinking(false);
            }
          }}
        />
      ) : null}

      {scanNote ? (
        <p className={`scan-note ${scanState === "error" ? "error" : ""}`} role={scanState === "error" ? "alert" : "status"}>
          {scanNote}
        </p>
      ) : null}

      <section className="card-list" aria-label={t("nav.skills")}>
        {libraryGroups.series.map((series) =>
          renderSeries(series, "library", seriesSummary(t, series, currentTool), libraryCard),
        )}
        {libraryGroups.singles.map(libraryCard)}
        {visibleLibrary.length === 0 && scanState !== "scanning" ? (
          <div className="empty-state">
            <strong>{library.length === 0 ? t("empty.noSkillsTitle") : t("empty.noMatchTitle")}</strong>
            <span>{library.length === 0 ? t("empty.noSkillsBody") : t("empty.noMatchBody")}</span>
          </div>
        ) : null}
      </section>

      {toolLocals.length > 0 ? (
        <section className="card-list" aria-label={currentTool ? t("locals.headingTool", { tool: currentTool.name }) : t("locals.headingAll")}>
          <div className="divider-label">
            <span>{currentTool ? t("locals.headingTool", { tool: currentTool.name }) : t("locals.headingAll")}</span>
          </div>
          {localGroups.series.map((series) =>
            renderSeries(series, "local", series.url.replace(/^https?:\/\//, ""), localCard),
          )}
          {localGroups.singles.map(localCard)}
        </section>
      ) : null}
    </div>
  );
}

const EXPANDED_KEY = "skills-manage.series.expanded";

/** Which series are open is a per-viewer convenience; storage may be unavailable. */
function readExpanded(): Set<string> {
  try {
    const saved: unknown = JSON.parse(window.localStorage.getItem(EXPANDED_KEY) ?? "[]");
    return new Set(Array.isArray(saved) ? saved.filter((item): item is string => typeof item === "string") : []);
  } catch {
    return new Set();
  }
}

function writeExpanded(keys: Set<string>) {
  try {
    window.localStorage.setItem(EXPANDED_KEY, JSON.stringify([...keys]));
  } catch {
    // Not remembered across launches; still works for this session.
  }
}

/** "12 · Claude Code 3/12 · Codex 5/12", or just the current tool's count. */
function seriesSummary(t: TFunction, series: SkillSeries, currentTool: (typeof tools)[number] | undefined): string {
  const total = series.skills.length;
  const parts = (currentTool ? [currentTool] : tools).map((tool) =>
    t("series.enabledIn", { tool: tool.name, count: enabledCount(series.skills, tool.id), total }),
  );
  return parts.join(" · ");
}

function SeriesGroup({
  id,
  t,
  series,
  summary,
  open,
  canToggle,
  busy,
  onToggle,
  onOpenUrl,
  onUnlink,
  children,
}: {
  id: string;
  t: TFunction;
  series: SkillSeries;
  summary: string;
  open: boolean;
  canToggle: boolean;
  busy: boolean;
  onToggle: () => void;
  onOpenUrl: (url: string) => void;
  onUnlink: (repository: string) => void;
  children: ReactNode;
}) {
  const listId = `series-${id.replace(/[^a-zA-Z0-9-]/g, "-")}`;
  return (
    <div className={`series ${open ? "open" : ""}`}>
      <div className="series-head">
        <button
          type="button"
          className="series-toggle"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-label={t(open ? "series.collapse" : "series.expand", { repo: series.repository })}
          disabled={!canToggle}
          onClick={onToggle}
        >
          <ChevronRight className="series-chevron" size={16} strokeWidth={2.2} aria-hidden="true" />
          <span className="series-tile" aria-hidden="true">
            <FolderGit2 size={17} strokeWidth={1.9} />
          </span>
          <span className="card-main">
            <span className="card-title">
              <strong>{series.repository}</strong>
              <span className="tag neutral">{t("series.count", { count: series.skills.length })}</span>
            </span>
            <span className="card-sub">{summary}</span>
          </span>
        </button>
        {series.linked ? (
          <button
            type="button"
            className="icon-button"
            aria-label={t("series.unlink", { repo: series.repository })}
            title={t("series.unlink", { repo: series.repository })}
            disabled={busy}
            onClick={() => onUnlink(series.repository)}
          >
            <Unlink size={15} strokeWidth={1.9} aria-hidden="true" />
          </button>
        ) : null}
        <button
          type="button"
          className="button outline small"
          aria-label={t("series.openGithubLabel", { repo: series.repository })}
          title={series.url}
          onClick={() => onOpenUrl(series.url)}
        >
          <ExternalLink size={13} strokeWidth={2} aria-hidden="true" />
          {t("series.openGithub")}
        </button>
      </div>
      {open ? (
        <div className="series-members" id={listId}>
          {children}
        </div>
      ) : null}
    </div>
  );
}

function LinkSeriesPanel({
  t,
  busy,
  onCancel,
  onSubmit,
}: {
  t: TFunction;
  busy: boolean;
  onCancel: () => void;
  onSubmit: (url: string) => Promise<void>;
}) {
  const [url, setUrl] = useState("");
  const [pending, setPending] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!url.trim() || pending) {
      return;
    }
    setPending(true);
    try {
      await onSubmit(url.trim());
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="link-panel" aria-label={t("series.linkTitle")} onSubmit={(event) => void submit(event)}>
      <strong>{t("series.linkTitle")}</strong>
      <span className="field-hint">{t("series.linkHint")}</span>
      <div className="link-row">
        <input
          className="text-input mono"
          type="url"
          aria-label={t("series.linkTitle")}
          placeholder={t("series.linkPlaceholder")}
          value={url}
          autoFocus
          onChange={(event) => setUrl(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              onCancel();
            }
          }}
        />
        <button type="button" className="button ghost" onClick={onCancel}>
          {t("actions.cancel")}
        </button>
        <button type="submit" className="button primary" disabled={!url.trim() || pending || busy}>
          {pending ? t("series.linking") : t("series.linkSubmit")}
        </button>
      </div>
    </form>
  );
}

function LocalCard({ t, skill, busyKey, onAddToLibrary, onOpenUrl, home }: Props & { skill: SkillRecord }) {
  const github = githubSource(skill);
  return (
    <article className="skill-card local">
      <SkillTile name={skill.name} muted />
      <div className="card-main">
        <span className="card-title">
          <strong>{skill.name}</strong>
          <span className="tag neutral">{t("locals.onlyIn", { tool: skill.source })}</span>
        </span>
        {github ? (
          <a
            className="card-sub link"
            href={github}
            title={tildePath(skill.sourcePath, home)}
            onClick={(event) => {
              event.preventDefault();
              onOpenUrl(github);
            }}
          >
            {github.replace(/^https?:\/\//, "")}
          </a>
        ) : (
          <span className="card-sub">{tildePath(skill.sourcePath, home)}</span>
        )}
      </div>
      <button
        type="button"
        className="button outline"
        aria-label={t("actions.addSkillToLibrary", { skill: skill.name })}
        disabled={busyKey !== null}
        onClick={() => onAddToLibrary(skill)}
      >
        {t("actions.addToLibrary")}
      </button>
    </article>
  );
}

function SkillCard({
  t,
  tab,
  skill,
  updates,
  busyKey,
  onToggle,
  onOpen,
  onOpenFolder,
  onOpenUrl,
  onConflict,
  home,
}: Props & { skill: SkillRecord }) {
  const broken = skill.health === "broken";
  const update = updates.get(skill.sourcePath);
  const source = sourceLine(skill, home);
  const issues = skill.issues ?? [];
  const currentTool = tools.find((tool) => tool.id === tab);
  const enabledHere = currentTool ? isEnabledIn(skill, currentTool.id) : false;
  const conflictHere = currentTool ? hasConflictIn(skill, currentTool.id) : false;
  const busy = busyKey !== null;

  return (
    <article className={`skill-card ${broken ? "broken" : ""}`}>
      <SkillTile name={skill.name} />
      <div className="card-main">
        <span className="card-title">
          <button type="button" className="name-button" onClick={() => onOpen(skill)}>
            {skill.name}
          </button>
          {enabledHere ? <span className="tag success">{t("card.enabled")}</span> : null}
          {update?.hasUpdate ? <span className="tag info">{t("card.updateAvailable")}</span> : null}
          {issues.includes("missing-name") ? <span className="tag warning">{t("card.missingName")}</span> : null}
          {issues.includes("missing-description") ? (
            <span className="tag warning">{t("card.missingDescription")}</span>
          ) : null}
          {skill.health === "warning" && issues.length === 0 ? (
            <span className="tag warning">{t("card.needsCheck")}</span>
          ) : null}
        </span>
        {broken ? (
          <span className="card-sub problem">{t("card.broken")}</span>
        ) : source.kind === "github" ? (
          <a
            className="card-sub link"
            href={source.url}
            onClick={(event) => {
              event.preventDefault();
              onOpenUrl(source.url);
            }}
          >
            {source.text}
          </a>
        ) : (
          <button type="button" className="card-sub link as-text" onClick={() => onOpen(skill)}>
            {source.kind === "local"
              ? t("card.originLocal", { path: source.path })
              : source.kind === "skillpack"
                ? t("card.originPack", { name: source.name })
                : t("card.libraryPath", { path: source.folder })}
          </button>
        )}
      </div>

      <span className="hover-actions">
        <button
          type="button"
          className="icon-button"
          aria-label={t("actions.editSkill", { skill: skill.name })}
          title={t("actions.edit")}
          onClick={() => onOpen(skill)}
        >
          <Pencil size={15} strokeWidth={1.9} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={t("actions.openSkillFolder", { skill: skill.name })}
          title={t("actions.openFolder")}
          onClick={() => onOpenFolder(skill.sourcePath)}
        >
          <FolderOpen size={15} strokeWidth={1.9} aria-hidden="true" />
        </button>
      </span>

      {currentTool && conflictHere && !broken ? (
        <button
          type="button"
          className="button conflict toggle-button"
          aria-label={t("actions.resolveConflict", { skill: skill.name, tool: currentTool.name })}
          disabled={busy}
          onClick={() => onConflict(skill, currentTool.id)}
        >
          {t("card.conflict", { tool: currentTool.name })}
        </button>
      ) : currentTool ? (
        <button
          type="button"
          className={`button ${broken ? "" : enabledHere ? "quiet" : "outline"} toggle-button`}
          aria-label={
            broken
              ? t("actions.needsFix", { skill: skill.name })
              : t(enabledHere ? "actions.disableIn" : "actions.enableIn", { skill: skill.name, tool: currentTool.name })
          }
          disabled={broken || busy}
          onClick={() => onToggle(skill, currentTool.id, !enabledHere)}
        >
          {broken ? t("actions.cannotEnable") : enabledHere ? t("actions.disable") : t("actions.enable")}
        </button>
      ) : (
        <span className="tool-pills" role="group" aria-label={t("card.locations", { skill: skill.name })}>
          {tools.map((tool) => {
            const on = isEnabledIn(skill, tool.id);
            if (hasConflictIn(skill, tool.id) && !broken) {
              return (
                <button
                  type="button"
                  key={tool.id}
                  className="tool-pill conflict"
                  aria-label={t("actions.resolveConflict", { skill: skill.name, tool: tool.name })}
                  disabled={busy}
                  onClick={() => onConflict(skill, tool.id)}
                >
                  <ToolMark mono={tool.mono} color="#d97706" round />
                  {t("card.conflict", { tool: tool.name })}
                </button>
              );
            }
            return (
              <button
                type="button"
                key={tool.id}
                className={`tool-pill ${on && !broken ? "on" : ""}`}
                aria-pressed={on}
                aria-label={
                  broken
                    ? t("actions.needsFix", { skill: skill.name })
                    : t(on ? "actions.disableIn" : "actions.enableIn", { skill: skill.name, tool: tool.name })
                }
                disabled={broken || busy}
                onClick={() => onToggle(skill, tool.id, !on)}
              >
                <ToolMark mono={tool.mono} color={on && !broken ? tool.color : "#cbd0d6"} round />
                {tool.name}
                <Check className="pill-check" size={12} strokeWidth={3} aria-hidden="true" />
              </button>
            );
          })}
        </span>
      )}
    </article>
  );
}
