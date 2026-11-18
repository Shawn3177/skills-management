import { useEffect, useRef, useState } from "react";
import { ExternalLink, Search } from "lucide-react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { TFunction } from "../i18n/useLocale";
import {
  api,
  errorText,
  type PublicPreview,
  type PublicResolution,
  type PublicSearchResults,
  type PublicSkill,
} from "../lib/api";
import { skillBody } from "./DetailView";

type Props = {
  t: TFunction;
  working: boolean;
  onSelected: (preview: PublicPreview | null) => void;
  onOpenUrl: (url: string) => void;
};

export function PublicSkillPicker({
  t,
  working,
  onSelected,
  onOpenUrl,
}: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<PublicSearchResults | null>(null);
  const [selected, setSelected] = useState<PublicSkill | null>(null);
  const [resolution, setResolution] = useState<PublicResolution | null>(null);
  const [path, setPath] = useState<string | null>(null);
  const [preview, setPreview] = useState<PublicPreview | null>(null);
  const [searching, setSearching] = useState(false);
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const searchRequest = useRef(0);
  const detailRequest = useRef(0);
  const previewElement = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!preview) return;
    previewElement.current?.focus({ preventScroll: true });
    previewElement.current?.scrollIntoView?.({ block: "start" });
  }, [preview]);

  useEffect(
    () => () => {
      searchRequest.current++;
      detailRequest.current++;
    },
    [],
  );

  function resetDetail() {
    detailRequest.current++;
    setSelected(null);
    setResolution(null);
    setPath(null);
    setPreview(null);
    setReading(false);
    onSelected(null);
  }

  async function search() {
    if (working || searching || query.trim().length < 2) return;
    const id = ++searchRequest.current;
    resetDetail();
    setResults(null);
    setError(null);
    setSearching(true);
    try {
      const next = await api.searchPublicSkills(query.trim());
      if (id === searchRequest.current) setResults(next);
    } catch (reason) {
      if (id === searchRequest.current) setError(errorText(reason));
    } finally {
      if (id === searchRequest.current) setSearching(false);
    }
  }

  async function read(
    repository: PublicResolution,
    selectedPath: string,
    id: number,
  ) {
    setPath(selectedPath);
    setPreview(null);
    onSelected(null);
    setError(null);
    setReading(true);
    try {
      const next = await api.previewPublicSkill(
        repository.repositoryId,
        selectedPath,
      );
      if (id !== detailRequest.current) return;
      setPreview(next);
      onSelected(next);
    } catch (reason) {
      if (id === detailRequest.current) setError(errorText(reason));
    } finally {
      if (id === detailRequest.current) setReading(false);
    }
  }

  async function select(skill: PublicSkill) {
    resetDetail();
    const id = detailRequest.current;
    setSelected(skill);
    setReading(true);
    setError(null);
    try {
      const next = await api.resolvePublicSkill(skill.source, skill.skillId);
      if (id !== detailRequest.current) return;
      setResolution(next);
      if (next.suggestedPath !== null) await read(next, next.suggestedPath, id);
    } catch (reason) {
      if (id === detailRequest.current) setError(errorText(reason));
    } finally {
      if (id === detailRequest.current) setReading(false);
    }
  }

  return (
    <div className="public-discovery">
      <label className="field-group">
        <span className="field-label">{t("discover.query")}</span>
        <span className="public-search-row">
          <input
            className="text-input"
            maxLength={120}
            value={query}
            disabled={working}
            aria-label={t("discover.query")}
            placeholder={t("discover.placeholder")}
            onChange={(event) => {
              setQuery(event.target.value);
              searchRequest.current++;
              setSearching(false);
              setResults(null);
              setError(null);
              resetDetail();
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                event.preventDefault();
                event.stopPropagation();
                void search();
              }
            }}
          />
          <button
            type="button"
            className="button"
            disabled={working || searching || query.trim().length < 2}
            onClick={() => void search()}
          >
            <Search size={15} aria-hidden="true" />
            {t(searching ? "discover.searching" : "discover.search")}
          </button>
        </span>
      </label>
      {error ? (
        <div className="notice error" role="alert">
          {error}
        </div>
      ) : null}
      {searching ? (
        <p role="status" className="field-hint">
          {t("discover.searching")}
        </p>
      ) : null}
      {results ? (
        <>
          <p role="status" className="field-hint">
            {t(results.skills.length ? "discover.count" : "discover.empty", {
              count: results.skills.length,
            })}
          </p>
          {results.unsupported > 0 ? (
            <p className="field-hint">
              {t("discover.unsupported", { count: results.unsupported })}
            </p>
          ) : null}
          <div className="public-results" aria-label={t("discover.results")}>
            {results.skills.map((skill) => (
              <button
                type="button"
                key={skill.id}
                className={`public-result ${selected?.id === skill.id ? "selected" : ""}`}
                disabled={working}
                aria-label={`${skill.name} · ${skill.source}`}
                aria-pressed={selected?.id === skill.id}
                onClick={() => void select(skill)}
              >
                <span>
                  <strong>{skill.name}</strong>
                  <span className="muted small">{skill.source}</span>
                </span>
                <span className="muted small">
                  {t("discover.installs", {
                    count: skill.installs.toLocaleString(),
                  })}
                </span>
              </button>
            ))}
          </div>
        </>
      ) : null}
      {resolution ? (
        <label className="field-group">
          <span className="field-label">{t("discover.folder")}</span>
          <select
            className="text-input"
            disabled={working}
            value={path ?? "__choose__"}
            aria-label={t("discover.folder")}
            onChange={(event) =>
              void read(resolution, event.target.value, ++detailRequest.current)
            }
          >
            <option value="__choose__" disabled>
              {t("discover.chooseFolder")}
            </option>
            {resolution.paths.map((folder) => (
              <option key={folder} value={folder}>
                {folder || t("discover.root")}
              </option>
            ))}
          </select>
          {resolution.suggestedPath === null ? (
            <span className="field-hint">{t("discover.ambiguous")}</span>
          ) : null}
        </label>
      ) : null}
      {reading ? (
        <p role="status" className="field-hint">
          {t("discover.reading")}
        </p>
      ) : null}
      {preview ? (
        <section className="public-preview" aria-label={t("discover.preview")} ref={previewElement} tabIndex={-1}>
          <div className="public-preview-heading">
            <strong>{preview.name}</strong>
            {preview.installedPath ? (
              <span className="tag success">{t("discover.installed")}</span>
            ) : null}
            <button
              type="button"
              className="button small"
              onClick={() => onOpenUrl(preview.url)}
            >
              <ExternalLink size={13} aria-hidden="true" />
              {t("discover.source")}
            </button>
          </div>
          <p className="muted">{preview.description}</p>
          <p className="field-hint mono">
            {preview.source} / {preview.path || t("discover.root")} ·{" "}
            {preview.commit.slice(0, 7)}
          </p>
          <p className="field-hint">
            {t(
              preview.installedPath
                ? "discover.reuseHint"
                : "discover.previewHint",
            )}
          </p>
          <details>
            <summary>
              {t("discover.files", { count: preview.files.length })}
            </summary>
            <ul className="public-files mono">
              {preview.files.map((file) => (
                <li key={file}>{file}</li>
              ))}
            </ul>
          </details>
          <div className="markdown-body public-markdown" tabIndex={0}>
            <Markdown
              remarkPlugins={[remarkGfm]}
              skipHtml
              components={{
                img: ({ alt }) => <span className="muted">{alt}</span>,
                a: ({ href, children }) =>
                  /^https?:\/\//i.test(href ?? "") ? (
                    <a
                      href={href}
                      onClick={(event) => {
                        event.preventDefault();
                        onOpenUrl(href!);
                      }}
                    >
                      {children}
                    </a>
                  ) : (
                    <span>{children}</span>
                  ),
              }}
            >
              {skillBody(preview.markdown)}
            </Markdown>
          </div>
        </section>
      ) : null}
    </div>
  );
}
