import { type ReactNode } from "react";
import {
  Archive,
  Boxes,
  CheckCircle2,
  ChevronDown,
  CircleAlert,
  CircleX,
  Download,
  FolderInput,
  GitFork,
  Package,
  Puzzle,
  ShieldCheck,
} from "lucide-react";
import type { MessageKey } from "../i18n/messages";
import type { TFunction } from "../i18n/useLocale";
import { isImportableSkill } from "../lib/skills";
import type { SkillHealth, SkillOriginKind, SkillRecord } from "../lib/skills";

export type ImportState = "idle" | "importing" | "success" | "error";
export type TargetActionState = "idle" | "saving" | "success" | "error";

export const healthLabelKeys: Record<SkillHealth, MessageKey> = {
  healthy: "health.healthy",
  warning: "health.warning",
  broken: "health.broken",
};

export const healthIcons = {
  healthy: CheckCircle2,
  warning: CircleAlert,
  broken: CircleX,
};

export const originLabelKeys: Record<SkillOriginKind, MessageKey> = {
  standalone: "origin.standalone",
  collection: "origin.collection",
  plugin: "origin.plugin",
  system: "origin.system",
};

export const originIcons = {
  standalone: Package,
  collection: GitFork,
  plugin: Puzzle,
  system: ShieldCheck,
};

export const toggleableTargets = [
  { id: "codex", name: "Codex" },
  { id: "claude-code", name: "Claude Code" },
  { id: "vs-code", name: "VS Code" },
];
export const toggleableTargetIds = new Set(toggleableTargets.map((target) => target.id));

export function hasSource(skill: SkillRecord, source: string) {
  return skill.source
    .split(",")
    .map((entry) => entry.trim())
    .includes(source);
}

export function SkillSourceGroup({
  name,
  description,
  kind,
  skillCount,
  expanded,
  onToggle,
  children,
  t,
}: {
  name: string;
  description: string;
  kind: Exclude<SkillOriginKind, "standalone">;
  skillCount: number;
  expanded: boolean;
  onToggle: () => void;
  children: ReactNode;
  t: TFunction;
}) {
  const actionLabel = t(expanded ? "actions.collapseCollection" : "actions.expandCollection", { name });
  const OriginIcon = originIcons[kind];

  return (
    <section className={`skill-source-group ${kind} ${expanded ? "expanded" : ""}`} aria-label={name}>
      <button
        className="skill-source-group-header"
        type="button"
        aria-expanded={expanded}
        aria-label={actionLabel}
        onClick={onToggle}
      >
        <span className={`skill-source-group-icon ${kind}`}>
          <OriginIcon size={18} strokeWidth={1.8} aria-hidden="true" />
        </span>
        <span className="skill-source-group-main">
          <strong>{name}</strong>
          <span title={description}>{description}</span>
        </span>
        <span className="skill-source-group-count">{t("collection.skillCount", { count: skillCount })}</span>
        <ChevronDown className="skill-source-group-chevron" size={18} strokeWidth={1.8} aria-hidden="true" />
      </button>
      {expanded ? <div className="skill-source-group-items">{children}</div> : null}
    </section>
  );
}

export function SkillListSection({
  kind,
  title,
  summary,
  children,
}: {
  kind: SkillOriginKind;
  title: string;
  summary: string;
  children: ReactNode;
}) {
  const OriginIcon = originIcons[kind];

  return (
    <section className="skill-list-section" aria-label={title}>
      <header className="skill-list-section-header">
        <span className={`skill-list-section-icon ${kind}`}>
          <OriginIcon size={15} strokeWidth={1.9} aria-hidden="true" />
        </span>
        <h2>{title}</h2>
        <span className="skill-list-section-summary">{summary}</span>
      </header>
      <div className="skill-list-section-items">{children}</div>
    </section>
  );
}

export function SkillListItem({
  skill,
  selected,
  onSelect,
  usageLabels,
}: {
  skill: SkillRecord;
  selected: boolean;
  onSelect: () => void;
  usageLabels: string[];
}) {
  const Icon = healthIcons[skill.health];

  return (
    <button
      className={`skill-row ${selected ? "selected" : ""}`}
      type="button"
      data-skill-id={skill.id}
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
    >
      <span className={`health-dot ${skill.health}`}>
        <Icon size={16} strokeWidth={1.9} />
      </span>
      <span className="skill-row-main">
        <strong>{skill.name}</strong>
        <span>{skill.description}</span>
      </span>
      {usageLabels.length > 0 ? (
        <span className="skill-row-meta">
          {usageLabels.map((label) => (
            <span className="usage-pill" key={label}>
              {label}
            </span>
          ))}
        </span>
      ) : null}
    </button>
  );
}

export function ToggleSwitch({
  checked,
  disabled,
  busy,
  label,
  onToggle,
}: {
  checked: boolean;
  disabled?: boolean;
  busy?: boolean;
  label: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-busy={busy || undefined}
      className={`toggle-switch ${checked ? "on" : ""}`}
      disabled={disabled}
      onClick={onToggle}
    >
      <span className="toggle-track">
        <span className="toggle-knob" />
      </span>
    </button>
  );
}

export function SkillDetail({
  skill,
  importDisabled,
  importMessage,
  importState,
  onExport,
  onImport,
  onRepair,
  onTargetBlocked,
  onToggleTarget,
  targetActionMessage,
  targetActionState,
  savingKey,
  targetToggleLocked,
  utilityMessage,
  t,
}: {
  skill: SkillRecord;
  importDisabled: boolean;
  importMessage: string;
  importState: ImportState;
  onExport: () => void;
  onImport: () => void;
  onRepair: () => void;
  onTargetBlocked: (target: { id: string; name: string }) => void;
  onToggleTarget: (target: { id: string; name: string; enabled: boolean }) => void;
  targetActionMessage: string;
  targetActionState: TargetActionState;
  savingKey: string;
  targetToggleLocked: boolean;
  utilityMessage: string;
  t: TFunction;
}) {
  const Icon = healthIcons[skill.health];
  const originKindLabel = t(originLabelKeys[skill.origin.kind]);
  const importLabel =
    !isImportableSkill(skill)
      ? t("actions.managedByCodex")
      : hasSource(skill, "Shared Library")
      ? t("actions.alreadyInLibrary")
      : importState === "importing"
        ? t("actions.importing")
        : t("actions.importToLibrary");

  return (
    <div className="drawer-content">
      <header className="drawer-header">
        <div>
          <p className="eyebrow">{t("drawer.selectedSkill")}</p>
          <h2>{skill.name}</h2>
        </div>
        <span className={`health-badge ${skill.health}`}>
          <Icon size={16} strokeWidth={1.9} />
          {t(healthLabelKeys[skill.health])}
        </span>
      </header>

      <div className="drawer-body scroll-surface">
        <p className="detail-description">{skill.description}</p>

        <dl className="metadata-grid compact-metadata" aria-label={t("detail.metadata")}>
          <div>
            <dt>{t("detail.type")}</dt>
            <dd>{originKindLabel}</dd>
          </div>
          {skill.origin.plugin ? (
            <>
              <div>
                <dt>{t("detail.plugin")}</dt>
                <dd>{skill.origin.plugin.name}</dd>
              </div>
              <div>
                <dt>{t("detail.version")}</dt>
                <dd>{skill.origin.plugin.version}</dd>
              </div>
              <div>
                <dt>{t("detail.publisher")}</dt>
                <dd>{skill.origin.plugin.publisher}</dd>
              </div>
              {skill.origin.plugin.repository ? (
                <div>
                  <dt>{t("detail.repositoryUrl")}</dt>
                  <dd>{skill.origin.plugin.repository}</dd>
                </div>
              ) : null}
            </>
          ) : null}
          {skill.origin.repository ? (
            <>
              <div>
                <dt>{t("detail.repository")}</dt>
                <dd>{skill.origin.repository.name}</dd>
              </div>
              {skill.origin.repository.url ? (
                <div>
                  <dt>{t("detail.repositoryUrl")}</dt>
                  <dd>{skill.origin.repository.url}</dd>
                </div>
              ) : null}
            </>
          ) : null}
          <div>
            <dt>{t("detail.source")}</dt>
            <dd>{skill.source}</dd>
          </div>
          <div>
            <dt>{t("detail.path")}</dt>
            <dd>{skill.sourcePath}</dd>
          </div>
          <div>
            <dt>{t("detail.supportFiles")}</dt>
            <dd>{skill.supportFiles.join(", ")}</dd>
          </div>
        </dl>

        <div className="drawer-section-title">
          <h3 id="targets-heading">{t("detail.targets")}</h3>
          <ShieldCheck size={16} strokeWidth={1.8} aria-hidden="true" />
        </div>

        <section className="target-list" aria-labelledby="targets-heading">
          {skill.targets.map((target) => {
            const canToggle = hasSource(skill, "Shared Library") && toggleableTargetIds.has(target.id);
            const actionVerb = target.enabled ? t("actions.disable") : t("actions.enable");
            const actionLabel = `${actionVerb} ${target.name}`;
            const savingThisTarget = targetActionState === "saving" && savingKey === `${skill.id}::${target.id}`;
            const targetStatus =
              !hasSource(skill, "Shared Library")
                ? t("targets.unavailable")
                : !toggleableTargetIds.has(target.id)
                  ? t("targets.pending")
                  : target.enabled
                    ? t("targets.enabled")
                    : target.id === "vs-code" && skill.targets.some((entry) => entry.id === "claude-code" && entry.enabled)
                      ? t("targets.vsCodeViaClaude")
                      : t("targets.disabled");
            const handleTargetClick = () => {
              if (canToggle) {
                onToggleTarget(target);
                return;
              }

              onTargetBlocked(target);
            };

            return (
              <div className="target-row" key={target.id}>
                <span>
                  <strong>{target.name}</strong>
                  <small>{targetStatus}</small>
                </span>
                <button
                  type="button"
                  disabled={targetToggleLocked}
                  aria-label={actionLabel}
                  onClick={handleTargetClick}
                >
                  {savingThisTarget ? t("actions.saving") : actionVerb}
                </button>
              </div>
            );
          })}
        </section>
      </div>

      <section className="action-bar" aria-label={t("regions.skillActions")}>
        <button
          className="primary-action"
          type="button"
          disabled={importDisabled}
          onClick={onImport}
          aria-describedby={importMessage ? "import-status" : undefined}
        >
          <FolderInput size={17} strokeWidth={1.8} />
          {importLabel}
        </button>
        <button type="button" onClick={onRepair}>
          <Archive size={17} strokeWidth={1.8} />
          {t("actions.repair")}
        </button>
        <button type="button" onClick={onExport}>
          <Download size={17} strokeWidth={1.8} />
          {t("actions.exportSkillpack")}
        </button>
      </section>

      {importMessage ? (
        <div
          className={`import-status ${importState === "error" ? "error" : "success"}`}
          id="import-status"
          role={importState === "error" ? "alert" : "status"}
        >
          {importMessage}
        </div>
      ) : null}

      {targetActionMessage ? (
        <div
          className={`target-action-status ${targetActionState === "error" ? "error" : "success"}`}
          role={targetActionState === "error" ? "alert" : "status"}
        >
          {targetActionMessage}
        </div>
      ) : null}

      {utilityMessage ? <StatusMessage state="success" message={utilityMessage} /> : null}
    </div>
  );
}

export function WorkspacePanel({
  body,
  children,
  icon: Icon,
  label,
  title,
}: {
  body: string;
  children: ReactNode;
  icon: typeof Boxes;
  label: string;
  title: string;
}) {
  return (
    <section className="workspace-panel scroll-surface" aria-label={label}>
      <header className="workspace-panel-header">
        <span className="workspace-panel-icon">
          <Icon size={18} strokeWidth={1.8} />
        </span>
        <div>
          <h2>{title}</h2>
          <p>{body}</p>
        </div>
      </header>
      {children}
    </section>
  );
}

export function StatusMessage({ message, state }: { message: string; state: "success" | "error" }) {
  return (
    <div className={`utility-status ${state}`} role={state === "error" ? "alert" : "status"}>
      {message}
    </div>
  );
}
