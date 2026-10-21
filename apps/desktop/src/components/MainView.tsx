import {
  ChevronRight,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  Link2,
  ListPlus,
  Pencil,
  RefreshCw,
  Search,
  Star,
  Unlink,
} from "lucide-react";
import { useState, type FormEvent, type ReactNode } from "react";
import type { TFunction } from "../i18n/useLocale";
import type { SkillUpdateStatus } from "../lib/api";
import {
  allTools,
  enabledCount,
  folderName,
  githubSource,
  groupBySeries,
  hasConflictIn,
  isEnabledIn,
  tildePath,
  tools,
  type SkillRecord,
  type SkillSeries,
  type ToolId,
} from "../lib/skills";
import { GithubButton, SkillTile, ToolLogo } from "./ui";

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
  /** Show a skill's details (its description and SKILL.md). */
  onShowDetail: (skill: SkillRecord) => void;
  onOpenFolder: (path: string) => void;
  onOpenUrl: (url: string) => void;
  onAddToLibrary: (skill: SkillRecord) => void;
  /** A tool already has a different same-named folder: ask before replacing. */
  onConflict: (skill: SkillRecord, toolId: ToolId) => void;
  /** Group installed skills under a GitHub repository; resolves true on success. */
  onLinkSeries: (url: string) => Promise<boolean>;
  /** Look up unknown skill sources on GitHub; resolves true when it ran. */
  onFindSources: () => Promise<boolean>;
  /** Lower-cased `owner/repo` of every repository the connected account starred. */
  starred: Set<string>;
  onUnlinkSeries: (repository: string) => void;
  /** Add these tool-folder skills to the shared library (one per folder name). */
  onAddAllToLibrary: (skills: SkillRecord[]) => void;
  /** The user's home folder, to show paths as `~/…`. */
  home: string | null;
};

export function MainView(props: Props) {
  const { t, tab, library, visibleLibrary, locals, query, onQueryChange, scanState } = props;
  const currentTool = allTools.find((tool) => tool.id === tab);
  const summary = currentTool
    ? t("summary.tool", {
        total: library.length,
        tool: currentTool.name,
        count: enabledCount(library, currentTool.id),
      })
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
  const unsourced = toolLocals.filter((skill) => !githubSource(skill)).length;
  // Repositories grouped only because they were linked (the user can undo that).
  const linkedRepositories = [
    ...new Set(
      groupBySeries([...library, ...locals])
        .series.filter((series) => series.linked)
        .map((series) => series.repository),
    ),
  ];

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

  function renderSeries(
    series: SkillSeries,
    scope: string,
    card: (skill: SkillRecord) => ReactNode,
    action: ReactNode = null,
  ) {
    const key = `${scope}:${series.key}`;
    return (
      <SeriesGroup
        key={key}
        id={key}
        t={t}
        series={series}
        open={searching || expanded.has(key)}
        starred={props.starred.has(series.key)}
        canToggle={!searching}
        onToggle={() => toggleSeries(key)}
        onOpenUrl={props.onOpenUrl}
        action={action}
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
        <SourcesPanel
          t={t}
          busy={props.busyKey !== null}
          onCancel={() => setLinking(false)}
          onFind={async () => {
            if (await props.onFindSources()) {
              setLinking(false);
            }
          }}
          onLink={async (url) => {
            if (await props.onLinkSeries(url)) {
              setLinking(false);
            }
          }}
          linked={linkedRepositories}
          onUnlink={props.onUnlinkSeries}
        />
      ) : null}

      {scanNote ? (
        <p
          className={`scan-note ${scanState === "error" ? "error" : ""}`}
          role={scanState === "error" ? "alert" : "status"}
        >
          {scanNote}
        </p>
      ) : null}

      <section className="card-list" aria-label={t("nav.skills")}>
        {libraryGroups.series.map((series) =>
          renderSeries(series, "library", libraryCard),
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
        <section
          className="card-list"
          aria-label={currentTool ? t("locals.headingTool", { tool: currentTool.name }) : t("locals.headingAll")}
        >
          <div className="divider-label">
            <span>{currentTool ? t("locals.headingTool", { tool: currentTool.name }) : t("locals.headingAll")}</span>
            {unsourced > 0 && !linking ? (
              <button
                type="button"
                className="divider-action"
                disabled={props.busyKey !== null}
                onClick={() => void props.onFindSources()}
              >
                {props.busyKey === "series:find" ? t("series.finding") : t("series.unsourced", { count: unsourced })}
              </button>
            ) : null}
            <AddAllButton
              t={t}
              skills={toolLocals}
              busy={props.busyKey !== null}
              working={props.busyKey === "locals:add-all"}
              onConfirm={props.onAddAllToLibrary}
            />
          </div>
          {localGroups.series.map((series) =>
            renderSeries(
              series,
              "local",
              localCard,
              <span className="series-actions">
                <PlatformBadges t={t} name={series.repository} sources={series.skills.map((skill) => skill.source)} />
                <button
                  type="button"
                  className="icon-button add-icon"
                  aria-label={t("locals.addSeries", { repo: series.repository })}
                  title={t("locals.addSeries", { repo: series.repository })}
                  disabled={props.busyKey !== null}
                  onClick={() => props.onAddAllToLibrary(series.skills)}
                >
                  <FolderPlus size={16} strokeWidth={1.9} aria-hidden="true" />
                </button>
              </span>,
            ),
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

function SeriesGroup({
  id,
  t,
  series,
  open,
  starred,
  canToggle,
  onToggle,
  onOpenUrl,
  action,
  children,
}: {
  id: string;
  t: TFunction;
  series: SkillSeries;
  open: boolean;
  starred: boolean;
  canToggle: boolean;
  onToggle: () => void;
  onOpenUrl: (url: string) => void;
  /** Button on the right (e.g. add the whole series to the library). */
  action: ReactNode;
  children: ReactNode;
}) {
  const listId = `series-${id.replace(/[^a-zA-Z0-9-]/g, "-")}`;
  return (
    <div className={`series ${open ? "open" : ""}`}>
      <div className="series-head">
        <button
          type="button"
          className="series-toggle compact"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-label={t(open ? "series.collapse" : "series.expand", {
            repo: series.repository,
          })}
          disabled={!canToggle}
          onClick={onToggle}
        >
          <ChevronRight className="series-chevron" size={16} strokeWidth={2.2} aria-hidden="true" />
          <span className="series-tile" aria-hidden="true">
            <FolderGit2 size={17} strokeWidth={1.9} />
          </span>
        </button>
        <span className="card-main">
          <span className="card-title">
            <button type="button" className="name-button" tabIndex={-1} disabled={!canToggle} onClick={onToggle}>
              {series.repository}
            </button>
            {series.byPrefix ? <span className="muted small">{t("series.byPrefixShort")}</span> : null}
            {starred ? (
              <Star className="star-mark" size={13} strokeWidth={2.2} aria-label={t("mine.starred")} role="img" />
            ) : null}
            <span className="tag neutral">{t("series.count", { count: series.skills.length })}</span>
          </span>
          
        </span>
        {series.byPrefix ? null : (
          <GithubButton t={t} name={series.repository} url={series.url} onOpenUrl={onOpenUrl} />
        )}
        {action}
      </div>
      {open ? (
        <div className="series-members" id={listId}>
          {children}
        </div>
      ) : null}
    </div>
  );
}

function SourcesPanel({
  t,
  busy,
  onCancel,
  onFind,
  onLink,
  linked,
  onUnlink,
}: {
  t: TFunction;
  busy: boolean;
  onCancel: () => void;
  onFind: () => Promise<void>;
  onLink: (url: string) => Promise<void>;
  linked: string[];
  onUnlink: (repository: string) => void;
}) {
  const [url, setUrl] = useState("");
  const [pending, setPending] = useState<"find" | "link" | null>(null);

  async function run(kind: "find" | "link", action: () => Promise<void>) {
    setPending(kind);
    try {
      await action();
    } finally {
      setPending(null);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    if (url.trim() && !pending) {
      void run("link", () => onLink(url.trim()));
    }
  }

  return (
    <div
      className="link-panel"
      role="region"
      aria-label={t("series.panelTitle")}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          onCancel();
        }
      }}
    >
      <div className="link-panel-head">
        <strong>{t("series.panelTitle")}</strong>
        <button type="button" className="button ghost small" onClick={onCancel}>
          {t("actions.cancel")}
        </button>
      </div>
      <span className="field-hint">{t("series.findHint")}</span>
      <div className="link-row">
        <button
          type="button"
          className="button primary"
          autoFocus
          disabled={pending !== null || busy}
          onClick={() => void run("find", onFind)}
        >
          <Search size={14} strokeWidth={2} aria-hidden="true" />
          {pending === "find" ? t("series.finding") : t("series.findButton")}
        </button>
      </div>

      <form className="link-manual" onSubmit={submit}>
        <span className="field-hint">
          {t("series.manualHeading")} · {t("series.linkHint")}
        </span>
        <div className="link-row">
          <input
            className="text-input mono"
            type="url"
            aria-label={t("series.linkTitle")}
            placeholder={t("series.linkPlaceholder")}
            value={url}
            onChange={(event) => setUrl(event.currentTarget.value)}
          />
          <button type="submit" className="button outline" disabled={!url.trim() || pending !== null || busy}>
            {pending === "link" ? t("series.linking") : t("series.linkSubmit")}
          </button>
        </div>
      </form>

      {linked.length > 0 ? (
        <div className="link-manual">
          <span className="field-hint">{t("series.linkedList")}</span>
          <div className="chip-row">
            {linked.map((repository) => (
              <span className="linked-chip" key={repository}>
                {repository}
                <button
                  type="button"
                  className="icon-button tiny"
                  aria-label={t("series.unlink", { repo: repository })}
                  title={t("series.unlink", { repo: repository })}
                  disabled={busy}
                  onClick={() => onUnlink(repository)}
                >
                  <Unlink size={12} strokeWidth={2} aria-hidden="true" />
                </button>
              </span>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** The tools a skill (or a series) currently lives in, as bare logos. */
function PlatformBadges({ t, name, sources }: { t: TFunction; name: string; sources: string[] }) {
  const unique = [...new Set(sources)];
  const known = tools.filter((tool) => unique.includes(tool.sourceLabel));
  const others = unique.filter((source) => !tools.some((tool) => tool.sourceLabel === source));
  const names = [...known.map((tool) => tool.name), ...others];
  return (
    <span className="platform-icons" role="img" aria-label={t("locals.inTools", { skill: name, tools: names.join(", ") })}>
      {known.map((tool) => (
        <span key={tool.id} title={tool.name}>
          <ToolLogo id={tool.id} />
        </span>
      ))}
      {others.map((source) => (
        <span className="tag neutral" key={source}>
          {source}
        </span>
      ))}
    </span>
  );
}

/** "Add all to the shared library", asking once before copying many skills. */
function AddAllButton({
  t,
  skills,
  busy,
  working,
  onConfirm,
}: {
  t: TFunction;
  skills: SkillRecord[];
  busy: boolean;
  working: boolean;
  onConfirm: (skills: SkillRecord[]) => void;
}) {
  const [asking, setAsking] = useState(false);
  const count = new Set(skills.map((skill) => folderName(skill.sourcePath).toLowerCase())).size;
  if (count === 0) {
    return null;
  }
  if (asking) {
    return (
      <span className="confirm-group add-all">
        <span className="small">{t("locals.addAllAsk", { count })}</span>
        <button type="button" className="button small ghost" onClick={() => setAsking(false)}>
          {t("actions.cancel")}
        </button>
        <button
          type="button"
          className="button small primary"
          disabled={busy}
          onClick={() => {
            setAsking(false);
            onConfirm(skills);
          }}
        >
          {t("locals.addAllConfirm", { count })}
        </button>
      </span>
    );
  }
  return (
    <button
      type="button"
      className="icon-button add-icon add-all"
      aria-label={t("locals.addAll")}
      title={t("locals.addAll")}
      disabled={busy}
      onClick={() => setAsking(true)}
    >
      {working ? (
        <RefreshCw className="spin" size={15} strokeWidth={2} aria-hidden="true" />
      ) : (
        <ListPlus size={16} strokeWidth={1.9} aria-hidden="true" />
      )}
    </button>
  );
}

/** A click on a card's empty space opens the skill; its buttons keep their own job. */
function openOnCardClick(event: { target: EventTarget }, open: () => void) {
  if (!(event.target instanceof Element) || !event.target.closest("button, a, input, label")) {
    open();
  }
}

function LocalCard({ t, skill, busyKey, onAddToLibrary, onOpenUrl, onShowDetail, home }: Props & { skill: SkillRecord }) {
  const github = githubSource(skill);
  return (
    <article className="skill-card local clickable" onClick={(event) => openOnCardClick(event, () => onShowDetail(skill))}>
      <SkillTile name={skill.name} muted />
      <div className="card-main">
        <span className="card-title">
          <button type="button" className="name-button" onClick={() => onShowDetail(skill)}>
            {skill.name}
          </button>
        </span>
        <span className="card-sub" title={tildePath(skill.sourcePath, home)}>
          {skill.description}
        </span>
      </div>
      {github ? (
        <span className="hover-actions">
          <GithubButton t={t} name={skill.name} url={github} onOpenUrl={onOpenUrl} />
        </span>
      ) : null}
      <PlatformBadges t={t} name={skill.name} sources={[skill.source]} />
      <button
        type="button"
        className="icon-button add-icon"
        aria-label={t("actions.addSkillToLibrary", { skill: skill.name })}
        title={t("actions.addToLibrary")}
        disabled={busyKey !== null}
        onClick={() => onAddToLibrary(skill)}
      >
        <FolderPlus size={16} strokeWidth={1.9} aria-hidden="true" />
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
  onShowDetail,
}: Props & { skill: SkillRecord }) {
  const broken = skill.health === "broken";
  const github = githubSource(skill);
  const update = updates.get(skill.sourcePath);
  const issues = skill.issues ?? [];
  const currentTool = allTools.find((tool) => tool.id === tab);
  const enabledHere = currentTool ? isEnabledIn(skill, currentTool.id) : false;
  const conflictHere = currentTool ? hasConflictIn(skill, currentTool.id) : false;
  const busy = busyKey !== null;

  return (
    <article
      className={`skill-card clickable ${broken ? "broken" : ""}`}
      onClick={(event) => openOnCardClick(event, () => onShowDetail(skill))}
    >
      <SkillTile name={skill.name} />
      <div className="card-main">
        <span className="card-title">
          <button type="button" className="name-button" onClick={() => onShowDetail(skill)}>
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
        ) : (
          <span className="card-sub">{skill.description}</span>
        )}
      </div>

      <span className="hover-actions">
        {github ? <GithubButton t={t} name={skill.name} url={github} onOpenUrl={onOpenUrl} /> : null}
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
          aria-label={t("actions.resolveConflict", {
            skill: skill.name,
            tool: currentTool.name,
          })}
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
              : t(enabledHere ? "actions.disableIn" : "actions.enableIn", {
                  skill: skill.name,
                  tool: currentTool.name,
                })
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
                  className="tool-pill icon conflict"
                  title={t("card.conflict", { tool: tool.name })}
                  aria-label={t("actions.resolveConflict", {
                    skill: skill.name,
                    tool: tool.name,
                  })}
                  disabled={busy}
                  onClick={() => onConflict(skill, tool.id)}
                >
                  <ToolLogo id={tool.id} />
                  <span className="conflict-dot" aria-hidden="true" />
                </button>
              );
            }
            return (
              <button
                type="button"
                key={tool.id}
                className={`tool-pill icon ${on && !broken ? "on" : ""}`}
                title={tool.name}
                aria-pressed={on}
                aria-label={
                  broken
                    ? t("actions.needsFix", { skill: skill.name })
                    : t(on ? "actions.disableIn" : "actions.enableIn", {
                        skill: skill.name,
                        tool: tool.name,
                      })
                }
                disabled={broken || busy}
                onClick={() => onToggle(skill, tool.id, !on)}
              >
                <ToolLogo id={tool.id} dim={!on || broken} />
              </button>
            );
          })}
        </span>
      )}
    </article>
  );
}
