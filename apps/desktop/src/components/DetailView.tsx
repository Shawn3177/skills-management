import { FolderOpen, FolderPlus, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { TFunction } from "../i18n/useLocale";
import { api, errorText, isBridgeUnavailable } from "../lib/api";
import { githubSource, isEnabledIn, isLibrarySkill, tildePath, tools, type SkillRecord } from "../lib/skills";
import { GithubButton, SkillTile, ToolLogo } from "./ui";

type Props = {
  t: TFunction;
  skill: SkillRecord;
  home: string | null;
  busy: boolean;
  onOpenUrl: (url: string) => void;
  onOpenFolder: (path: string) => void;
  /** Library skills: open the edit page. */
  onEdit: (skill: SkillRecord) => void;
  /** Tool-folder skills: add to the shared library. */
  onAddToLibrary: (skill: SkillRecord) => void;
};

/** Everything after the frontmatter block of a SKILL.md. */
export function skillBody(markdown: string): string {
  const text = markdown.replace(/^﻿/, "");
  const match = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/.exec(text);
  return (match ? text.slice(match[0].length) : text).trim();
}

/** What a skill is and does: its description, where it lives, and its SKILL.md. */
export function DetailView({ t, skill, home, busy, onOpenUrl, onOpenFolder, onEdit, onAddToLibrary }: Props) {
  const [body, setBody] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const inLibrary = isLibrarySkill(skill);
  const github = githubSource(skill);
  const places = inLibrary
    ? tools.filter((tool) => isEnabledIn(skill, tool.id))
    : tools.filter((tool) => tool.sourceLabel === skill.source);

  useEffect(() => {
    let current = true;
    setBody(null);
    setError(null);
    api
      .readSkillMarkdown(skill.sourcePath)
      .then((markdown) => current && setBody(skillBody(markdown)))
      .catch((reason) => current && setError(isBridgeUnavailable(reason) ? t("errors.bridge") : errorText(reason)));
    return () => {
      current = false;
    };
  }, [skill.sourcePath, t]);

  return (
    <div className="page-body narrow">
      <section className="panel detail-head" aria-label={skill.name}>
        <div className="detail-title">
          <SkillTile name={skill.name} />
          <span className="card-main">
            <strong>{skill.name}</strong>
            <span className="detail-places">
              {places.length > 0 ? (
                places.map((tool) => (
                  <span className="tool-pill on" key={tool.id}>
                    <ToolLogo id={tool.id} />
                    {tool.name}
                  </span>
                ))
              ) : (
                <span className="muted small">{inLibrary ? t("detail.notEnabled") : skill.source}</span>
              )}
            </span>
          </span>
          <span className="detail-actions">
            {github ? <GithubButton t={t} name={skill.name} url={github} onOpenUrl={onOpenUrl} withLabel /> : null}
            <button type="button" className="button small" onClick={() => onOpenFolder(skill.sourcePath)}>
              <FolderOpen size={14} strokeWidth={1.9} aria-hidden="true" />
              {t("actions.openFolder")}
            </button>
            {inLibrary ? (
              <button type="button" className="button small primary" onClick={() => onEdit(skill)}>
                <Pencil size={14} strokeWidth={1.9} aria-hidden="true" />
                {t("actions.edit")}
              </button>
            ) : (
              <button type="button" className="button small primary" disabled={busy} onClick={() => onAddToLibrary(skill)}>
                <FolderPlus size={14} strokeWidth={1.9} aria-hidden="true" />
                {t("actions.addToLibrary")}
              </button>
            )}
          </span>
        </div>
        <p className="detail-description">{skill.description}</p>
        <dl className="detail-facts">
          <dt>{t("detail.location")}</dt>
          <dd className="mono">{tildePath(skill.sourcePath, home)}</dd>
          {github ? (
            <>
              <dt>{t("detail.source")}</dt>
              <dd className="mono">{github.replace(/^https?:\/\//, "")}</dd>
            </>
          ) : null}
          <dt>{t("detail.files")}</dt>
          <dd className="detail-files">
            {skill.supportFiles.map((file) => (
              <span className="chip static mono" key={file}>
                {file}
              </span>
            ))}
          </dd>
        </dl>
      </section>

      <section className="panel" aria-label="SKILL.md">
        <h2>SKILL.md</h2>
        {error ? <p className="field-hint">{error}</p> : null}
        {body === null && !error ? <p className="field-hint">{t("detail.loading")}</p> : null}
        {body !== null ? (
          <div className="markdown-body">
            {/* Raw HTML in SKILL.md is never rendered (react-markdown's default). */}
            <Markdown
              remarkPlugins={[remarkGfm]}
              components={{
                a: ({ href, children }) => (
                  <a
                    href={href}
                    onClick={(event) => {
                      event.preventDefault();
                      if (href && /^https?:\/\//.test(href)) {
                        onOpenUrl(href);
                      }
                    }}
                  >
                    {children}
                  </a>
                ),
                img: ({ alt }) => <span className="muted">[{alt || "image"}]</span>,
              }}
            >
              {body || t("detail.empty")}
            </Markdown>
          </div>
        ) : null}
      </section>
    </div>
  );
}
