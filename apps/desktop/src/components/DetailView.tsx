import { FolderOpen, FolderPlus, Pencil } from "lucide-react";
import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { TFunction } from "../i18n/useLocale";
import { api, errorText, isBridgeUnavailable, type CompatReport } from "../lib/api";
import { compatFor, compatPlace, compatReason } from "../lib/compat";
import {
  githubSource,
  isEnabledIn,
  isLibrarySkill,
  isManagedByCodex,
  originKind,
  tildePath,
  tools,
  type SkillRecord,
  type ToolId,
  type VariantChoice,
} from "../lib/skills";
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
  /** A tool was marked as working for this skill: rescan. */
  onCompatChanged: () => void;
  /** A tool's version was switched, or versions were detected: rescan. */
  onVariantsChanged: () => void;
};

/** Everything after the frontmatter block of a SKILL.md. */
export function skillBody(markdown: string): string {
  const text = markdown.replace(/^﻿/, "");
  const match = /^---\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/.exec(text);
  return (match ? text.slice(match[0].length) : text).trim();
}

/** What a skill is and does: its description, where it lives, and its SKILL.md. */
export function DetailView({
  t,
  skill,
  home,
  busy,
  onOpenUrl,
  onOpenFolder,
  onEdit,
  onAddToLibrary,
  onCompatChanged,
  onVariantsChanged,
}: Props) {
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
            ) : isManagedByCodex(skill) ? (
              <button type="button" className="button small" disabled>
                {t("actions.managedByCodex")}
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
          <dt>{t("detail.type")}</dt>
          <dd>{t(`origin.${originKind(skill)}`)}</dd>
          {skill.origin?.plugin ? (
            <>
              <dt>{t("detail.plugin")}</dt>
              <dd>{skill.origin.plugin.name}</dd>
              <dt>{t("detail.version")}</dt>
              <dd className="mono">{skill.origin.plugin.version}</dd>
              <dt>{t("detail.publisher")}</dt>
              <dd>{skill.origin.plugin.publisher}</dd>
              {skill.origin.plugin.repository ? (
                <>
                  <dt>{t("detail.repositoryUrl")}</dt>
                  <dd className="mono">{skill.origin.plugin.repository}</dd>
                </>
              ) : null}
            </>
          ) : null}
          {skill.origin?.repository && !github ? (
            <>
              <dt>{t("detail.repository")}</dt>
              <dd>{skill.origin.repository.name}</dd>
            </>
          ) : null}
          <dt>{t("detail.location")}</dt>
          <dd className="mono">{tildePath(skill.sourcePath, home)}</dd>
          {github ? (
            <>
              <dt>{t("detail.source")}</dt>
              <dd className="mono">{github.replace(/^https?:\/\//, "")}</dd>
            </>
          ) : null}
          {inLibrary ? <VariantsFact t={t} skill={skill} onChanged={onVariantsChanged} /> : null}
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

      {inLibrary ? <CompatPanel t={t} skill={skill} onChanged={onCompatChanged} /> : null}

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

function toolLabel(targetId: string): string {
  return tools.find((tool) => tool.id === targetId)?.name ?? targetId;
}

/**
 * The tools with their own version of this library skill, each switchable
 * between that version and the main one. Skills added from GitHub can look
 * for such versions again (those added before they were kept have none).
 */
function VariantsFact({ t, skill, onChanged }: { t: TFunction; skill: SkillRecord; onChanged: () => void }) {
  const [working, setWorking] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const variants = skill.variants ?? [];
  const canDetect = skill.originKind === "github";
  if (variants.length === 0 && !canDetect) {
    return null;
  }

  async function run(key: string, action: () => Promise<string>) {
    setWorking(key);
    setStatus(key === "detect" ? t("detail.variantDetecting") : null);
    try {
      setStatus(await action());
      onChanged();
    } catch (reason) {
      setStatus(isBridgeUnavailable(reason) ? t("errors.bridge") : errorText(reason));
    } finally {
      setWorking(null);
    }
  }

  const choose = (targetId: ToolId, choice: VariantChoice) =>
    run(targetId, async () => {
      const result = await api.setVariantChoice(skill.sourcePath, targetId, choice);
      const tool = toolLabel(targetId);
      const text = t(choice === "default" ? "detail.variantUsingMain" : "detail.variantUsingOwn", { tool });
      return result.refreshed ? `${text} ${t("detail.variantSynced", { tool })}` : text;
    });

  const detect = () =>
    run("detect", async () => {
      const result = await api.detectVariants(skill.sourcePath);
      const list = (names: string[]) => names.join(t("detail.listSeparator"));
      const parts = [
        result.variants.length > 0
          ? t("detail.variantFound", { tools: list(result.variants.map((variant) => toolLabel(variant.targetId))) })
          : t("detail.variantNone"),
      ];
      if (result.refreshedTargets.length > 0) {
        parts.push(t("detail.variantRefreshed", { tools: list(result.refreshedTargets) }));
      }
      if (result.failedTargets.length > 0) {
        parts.push(t("detail.variantRefreshFailed", { tools: list(result.failedTargets) }));
      }
      return parts.join(" ");
    });

  return (
    <>
      <dt>{t("detail.variants")}</dt>
      <dd className="detail-variants">
        {variants.map((variant) => {
          const tool = toolLabel(variant.targetId);
          const choice = variant.choice ?? "variant";
          const options: Array<[VariantChoice, string]> = [
            ["variant", t("detail.variantOwn", { tool })],
            ["default", t("detail.variantMain")],
          ];
          return (
            <div className="variant-row" key={variant.targetId}>
              <span className="mono">{t("detail.variantItem", { tool, path: variant.path })}</span>
              <span className="segmented small" role="group" aria-label={t("detail.variantChoice", { tool })}>
                {options.map(([value, label]) => (
                  <button
                    type="button"
                    key={value}
                    className={choice === value ? "active" : undefined}
                    aria-pressed={choice === value}
                    disabled={working !== null}
                    onClick={() => choice !== value && void choose(variant.targetId as ToolId, value)}
                  >
                    {label}
                  </button>
                ))}
              </span>
            </div>
          );
        })}
        {canDetect ? (
          <span>
            <button
              type="button"
              className="button small quiet"
              title={t("detail.variantDetectHint")}
              disabled={working !== null}
              onClick={() => void detect()}
            >
              {t("detail.variantDetect")}
            </button>
          </span>
        ) : null}
        {status ? (
          <p className="field-hint" role="status">
            {status}
          </p>
        ) : null}
      </dd>
    </>
  );
}

/**
 * The shown tools this library skill may not suit, with what the static check
 * found, and a way to mark a tool as working so enabling there stops warning.
 */
function CompatPanel({ t, skill, onChanged }: { t: TFunction; skill: SkillRecord; onChanged: () => void }) {
  const [reports, setReports] = useState<Partial<Record<ToolId, CompatReport>>>({});
  const [working, setWorking] = useState<ToolId | null>(null);
  const [error, setError] = useState<string | null>(null);
  const entries = tools.flatMap((tool) => {
    const entry = compatFor(skill, tool.id);
    return entry ? [{ tool, entry }] : [];
  });
  if (entries.length === 0) {
    return null;
  }

  async function run(toolId: ToolId, action: () => Promise<void>) {
    setWorking(toolId);
    setError(null);
    try {
      await action();
    } catch (reason) {
      setError(isBridgeUnavailable(reason) ? t("errors.bridge") : errorText(reason));
    } finally {
      setWorking(null);
    }
  }

  const showAll = (toolId: ToolId) =>
    run(toolId, async () => {
      const report = await api.checkCompat(skill.sourcePath, toolId);
      setReports((current) => ({ ...current, [toolId]: report }));
    });

  const acknowledge = (toolId: ToolId) =>
    run(toolId, async () => {
      await api.acknowledgeCompat(skill.sourcePath, toolId);
      onChanged();
    });

  return (
    <section className="panel" aria-label={t("detail.compat")}>
      <h2>{t("detail.compat")}</h2>
      <p className="field-hint">{t("detail.compatHint")}</p>
      <ul className="compat-list">
        {entries.map(({ tool, entry }) => {
          const report = reports[tool.id];
          const findings = report ? report.findings : [entry.first];
          return (
            <li className="compat-row" key={tool.id}>
              <div className="compat-head">
                <span className="tool-pill on">
                  <ToolLogo id={tool.id} />
                  {tool.name}
                </span>
                <span className={`tag ${entry.acknowledged ? "neutral" : "warning"}`}>
                  {entry.acknowledged
                    ? t("detail.compatAcknowledged")
                    : t(entry.level === "incompatible" ? "detail.compatIncompatible" : "detail.compatDegraded")}
                </span>
                <span className="compat-actions">
                  {!report && entry.count > 1 ? (
                    <button
                      type="button"
                      className="button small quiet"
                      disabled={working !== null}
                      onClick={() => void showAll(tool.id)}
                    >
                      {t("detail.compatShowAll", { count: entry.count })}
                    </button>
                  ) : null}
                  {!entry.acknowledged ? (
                    <button
                      type="button"
                      className="button small"
                      disabled={working !== null}
                      onClick={() => void acknowledge(tool.id)}
                    >
                      {t("detail.compatAcknowledge")}
                    </button>
                  ) : null}
                </span>
              </div>
              {report?.declared ? <p className="muted small">{t("detail.compatDeclared", { text: report.declared })}</p> : null}
              <ul className="compat-findings">
                {findings.map((finding) => (
                  <li key={`${finding.ruleId}:${finding.token}`}>
                    <span>{compatReason(t, finding)}</span>
                    <span className="muted small mono">{compatPlace(t, finding)}</span>
                    {finding.excerpt ? <code className="compat-excerpt">{finding.excerpt}</code> : null}
                  </li>
                ))}
              </ul>
            </li>
          );
        })}
      </ul>
      {error ? <p className="field-hint">{error}</p> : null}
    </section>
  );
}
