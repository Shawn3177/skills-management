import { ChevronRight, ExternalLink, FolderGit2, Lock, RefreshCw, Search, Star } from "lucide-react";
import { useState } from "react";
import type { TFunction } from "../i18n/useLocale";
import type { CatalogRepo, GithubAccount, GithubCatalog } from "../lib/api";

type Filter = "all" | "owned" | "starred" | "skills";

type Props = {
  t: TFunction;
  locale: string;
  /** "add": only repositories with skills (Add skill page). "page": everything. */
  mode: "add" | "page";
  account: GithubAccount | null;
  catalog: GithubCatalog | null;
  loading: boolean;
  /** Lower-cased folder names already in the shared library. */
  libraryFolders: Set<string>;
  selected: Set<string>;
  onSelectedChange: (selected: Set<string>) => void;
  onRefresh: () => void;
  onConnect: () => void;
  onOpenUrl: (url: string) => void;
};

export function MyGithubPicker({
  t,
  locale,
  mode,
  account,
  catalog,
  loading,
  libraryFolders,
  selected,
  onSelectedChange,
  onRefresh,
  onConnect,
  onOpenUrl,
}: Props) {
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<Set<string>>(new Set());

  if (!account?.connected || !account.login) {
    return (
      <div className="field-group">
        <span className="field-hint">{account?.connected ? t("mine.tokenBroken") : t("mine.notConnected")}</span>
        <span>
          <button type="button" className="button" disabled={loading} onClick={onConnect}>
            {t(mode === "page" ? "account.useCli" : "mine.connect")}
          </button>
        </span>
      </div>
    );
  }

  if (!catalog) {
    return <span className="field-hint">{loading ? t("mine.loading") : t("mine.empty")}</span>;
  }

  const normalized = query.trim().toLowerCase();
  const matchesFilter = (repo: CatalogRepo) =>
    filter === "owned"
      ? repo.owned
      : filter === "starred"
        ? repo.starred
        : filter === "skills" || mode === "add"
          ? repo.skills.length > 0
          : true;
  const repos = catalog.repos
    .filter(matchesFilter)
    .map((repo) => {
      const repoMatches =
        !normalized ||
        repo.fullName.toLowerCase().includes(normalized) ||
        (repo.description ?? "").toLowerCase().includes(normalized);
      const skills = repoMatches
        ? repo.skills
        : repo.skills.filter((skill) => skill.folder.toLowerCase().includes(normalized));
      return { repo, skills, visible: repoMatches || skills.length > 0 };
    })
    .filter((entry) => entry.visible && (mode === "page" || entry.skills.length > 0));

  const filters: Array<[Filter, string]> =
    mode === "page"
      ? [
          ["all", t("mine.filter.all")],
          ["owned", t("mine.filter.owned")],
          ["starred", t("mine.filter.starred")],
          ["skills", t("mine.filter.skills")],
        ]
      : [
          ["all", t("mine.filter.all")],
          ["owned", t("mine.filter.owned")],
          ["starred", t("mine.filter.starred")],
        ];
  const addable = (folder: string) => !libraryFolders.has(folder.toLowerCase());

  function setMany(urls: string[], on: boolean) {
    const next = new Set(selected);
    urls.forEach((url) => (on ? next.add(url) : next.delete(url)));
    onSelectedChange(next);
  }

  function toggleOpen(name: string) {
    setOpen((current) => {
      const next = new Set(current);
      if (!next.delete(name)) {
        next.add(name);
      }
      return next;
    });
  }

  const fetched = new Date(catalog.fetchedAt).toLocaleString(locale, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
  const withSkills = catalog.repos.filter((repo) => repo.skills.length > 0).length;

  return (
    <div className="field-group">
      <div className="mine-toolbar">
        <span className="muted small">
          {mode === "page"
            ? t("mine.summaryFull", {
                owned: catalog.repos.filter((repo) => repo.owned).length,
                starred: catalog.starred.length,
                skills: withSkills,
                time: fetched,
              })
            : t("mine.summary", { login: catalog.login, count: withSkills, time: fetched })}
          {catalog.rateLimited ? ` · ${t("mine.partial")}` : ""}
        </span>
        <span className="spacer" />
        <div className="segmented small" role="group" aria-label={t("mine.filter")}>
          {filters.map(([item, label]) => (
            <button
              type="button"
              key={item}
              aria-pressed={filter === item}
              className={filter === item ? "active" : ""}
              onClick={() => setFilter(item)}
            >
              {label}
            </button>
          ))}
        </div>
        <label className="search-field small">
          <Search size={13} strokeWidth={2} aria-hidden="true" />
          <input
            type="search"
            aria-label={t("mine.search")}
            placeholder={t("mine.search")}
            value={query}
            onChange={(event) => setQuery(event.currentTarget.value)}
          />
        </label>
        <button type="button" className="button small" disabled={loading} onClick={onRefresh}>
          <RefreshCw className={loading ? "spin" : ""} size={13} strokeWidth={2} aria-hidden="true" />
          {loading ? t("mine.loading") : t("mine.refresh")}
        </button>
      </div>

      {repos.length === 0 ? <span className="field-hint">{t("mine.none")}</span> : null}
      <div className="mine-list">
        {repos.map(({ repo, skills }) => (
          <RepoRow
            key={repo.fullName}
            t={t}
            locale={locale}
            repo={repo}
            skills={skills}
            open={(normalized !== "" && skills.length > 0) || open.has(repo.fullName)}
            selected={selected}
            addable={addable}
            onToggleOpen={() => toggleOpen(repo.fullName)}
            onSetMany={setMany}
            onOpenUrl={onOpenUrl}
          />
        ))}
      </div>
    </div>
  );
}

function RepoRow({
  t,
  locale,
  repo,
  skills,
  open,
  selected,
  addable,
  onToggleOpen,
  onSetMany,
  onOpenUrl,
}: {
  t: TFunction;
  locale: string;
  repo: CatalogRepo;
  skills: CatalogRepo["skills"];
  open: boolean;
  selected: Set<string>;
  addable: (folder: string) => boolean;
  onToggleOpen: () => void;
  onSetMany: (urls: string[], on: boolean) => void;
  onOpenUrl: (url: string) => void;
}) {
  const hasSkills = repo.skills.length > 0;
  const choosable = skills.filter((skill) => addable(skill.folder)).map((skill) => skill.url);
  const chosen = choosable.filter((url) => selected.has(url)).length;
  const listId = `mine-${repo.fullName.replace(/[^a-zA-Z0-9-]/g, "-")}`;
  const pushed = repo.pushedAt
    ? new Date(repo.pushedAt).toLocaleDateString(locale, { year: "numeric", month: "numeric", day: "numeric" })
    : null;
  const details = [
    repo.language,
    repo.stars > 0 ? `★ ${repo.stars.toLocaleString(locale)}` : null,
    pushed ? t("mine.pushed", { date: pushed }) : null,
  ].filter(Boolean);

  const title = (
    <span className="card-main">
      <span className="card-title">
        <strong>{repo.fullName}</strong>
        {repo.owned ? <span className="tag info">{t("mine.owned")}</span> : null}
        {repo.starred ? (
          <span className="tag neutral">
            <Star size={11} strokeWidth={2.2} aria-hidden="true" /> {t("mine.starred")}
          </span>
        ) : null}
        {repo.private ? (
          <span className="tag neutral">
            <Lock size={10} strokeWidth={2.2} aria-hidden="true" /> {t("mine.private")}
          </span>
        ) : null}
        {repo.fork ? <span className="tag neutral">{t("mine.fork")}</span> : null}
        <span className={`small ${hasSkills ? "skill-count" : "muted"}`}>
          {hasSkills ? t("mine.skillCount", { count: repo.skills.length }) : t("mine.noSkills")}
        </span>
      </span>
      {repo.description ? <span className="card-sub">{repo.description}</span> : null}
      {details.length > 0 ? <span className="mine-details">{details.join(" · ")}</span> : null}
    </span>
  );

  return (
    <div className={`mine-repo ${open ? "open" : ""} ${hasSkills ? "" : "plain"}`}>
      <div className="mine-repo-head">
        {hasSkills ? (
          <>
            <input
              type="checkbox"
              aria-label={t("mine.selectRepo", { repo: repo.fullName })}
              checked={choosable.length > 0 && chosen === choosable.length}
              ref={(element) => {
                if (element) {
                  element.indeterminate = chosen > 0 && chosen < choosable.length;
                }
              }}
              disabled={choosable.length === 0}
              onChange={(event) => onSetMany(choosable, event.currentTarget.checked)}
            />
            <button
              type="button"
              className="series-toggle"
              aria-expanded={open}
              aria-controls={open ? listId : undefined}
              onClick={onToggleOpen}
            >
              <ChevronRight className="series-chevron" size={15} strokeWidth={2.2} aria-hidden="true" />
              <FolderGit2 size={15} strokeWidth={1.9} aria-hidden="true" />
              {title}
            </button>
          </>
        ) : (
          <span className="series-toggle static">
            <FolderGit2 size={15} strokeWidth={1.9} aria-hidden="true" />
            {title}
          </span>
        )}
        <button
          type="button"
          className="icon-button"
          aria-label={t("series.openGithubLabel", { repo: repo.fullName })}
          title={repo.htmlUrl}
          onClick={() => onOpenUrl(repo.htmlUrl)}
        >
          <ExternalLink size={14} strokeWidth={1.9} aria-hidden="true" />
        </button>
      </div>
      {open && hasSkills ? (
        <div className="candidate-list mine-skills" id={listId}>
          {skills.map((skill) => {
            const inLibrary = !addable(skill.folder);
            return (
              <label className="candidate" key={skill.url}>
                <input
                  type="checkbox"
                  checked={inLibrary || selected.has(skill.url)}
                  disabled={inLibrary}
                  onChange={(event) => onSetMany([skill.url], event.currentTarget.checked)}
                />
                <span>
                  <strong>{skill.folder}</strong>
                  <small className="mono">{inLibrary ? t("mine.inLibrary") : skill.path || "/"}</small>
                </span>
              </label>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
