import { useState } from "react";
import { Check } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";
import { hasConflictIn, isEnabledIn, sourceLine, tildePath, tools, type SkillRecord, type ToolId } from "../lib/skills";
import { Chip, SkillTile } from "./ui";

export type EditChanges = {
  name: string;
  description: string;
  /** True when SKILL.md's name/description should be rewritten. */
  metadataChanged: boolean;
  /** Only the tools whose state should change. */
  toggles: Array<{ toolId: ToolId; enable: boolean }>;
};

type Props = {
  t: TFunction;
  skill: SkillRecord;
  home: string | null;
  busy: boolean;
  saving: boolean;
  onSave: (skill: SkillRecord, changes: EditChanges) => void;
  onExport: (skill: SkillRecord) => void;
  onRemove: (skill: SkillRecord) => void;
  onCancel: () => void;
};

export function EditView({ t, skill, home, busy, saving, onSave, onExport, onRemove, onCancel }: Props) {
  const broken = skill.health === "broken";
  const issues = skill.issues ?? [];
  const [name, setName] = useState(skill.name);
  const [description, setDescription] = useState(issues.includes("missing-description") ? "" : skill.description);
  const [enabled, setEnabled] = useState<Record<ToolId, boolean>>(
    () => Object.fromEntries(tools.map((tool) => [tool.id, isEnabledIn(skill, tool.id)])) as Record<ToolId, boolean>,
  );
  const [confirmingRemove, setConfirmingRemove] = useState(false);

  const toggles = tools
    .filter((tool) => enabled[tool.id] !== isEnabledIn(skill, tool.id))
    .map((tool) => ({ toolId: tool.id, enable: enabled[tool.id] }));
  const metadataChanged =
    name.trim() !== skill.name ||
    description.trim() !== (issues.includes("missing-description") ? "" : skill.description) ||
    issues.includes("missing-name");
  const dirty = metadataChanged || toggles.length > 0;
  const nameMissing = name.trim().length === 0;

  const source = sourceLine(skill, home);
  const sourceText =
    source.kind === "github"
      ? `GitHub · ${source.text}`
      : source.kind === "local"
        ? t("edit.fromLocal", { path: source.path })
        : source.kind === "skillpack"
          ? t("edit.fromPack", { name: source.name })
          : t("edit.fromLibrary");
  const missingFields = [
    issues.includes("missing-name") ? "name" : null,
    issues.includes("missing-description") ? "description" : null,
  ].filter(Boolean);

  function save() {
    if (!dirty || nameMissing || saving) {
      return;
    }
    onSave(skill, {
      name: metadataChanged ? name.trim() : skill.name,
      description: description.trim(),
      metadataChanged,
      toggles,
    });
  }

  return (
    <>
      <div className="page-body">
        <form
          className="panel"
          onSubmit={(event) => {
            event.preventDefault();
            save();
          }}
        >
          <div className="field-group">
            <span className="field-label" id="edit-enable-label">
              {t("edit.enableTo")}
            </span>
            <div className="chip-row" role="group" aria-labelledby="edit-enable-label">
              {tools.map((tool) => (
                <Chip
                  key={tool.id}
                  pressed={enabled[tool.id]}
                  disabled={broken}
                  onClick={() => setEnabled((current) => ({ ...current, [tool.id]: !current[tool.id] }))}
                >
                  {enabled[tool.id]
                    ? `✓ ${tool.name}`
                    : hasConflictIn(skill, tool.id)
                      ? t("card.conflict", { tool: tool.name })
                      : tool.name}
                </Chip>
              ))}
            </div>
            <span className="field-hint">{t("edit.enableHint")}</span>
          </div>

          <div className="hero-tile">
            <SkillTile name={name.trim() || skill.name} />
          </div>

          <div className="form-grid">
            <label className="field-group">
              <span className="field-label">{t("edit.name")}</span>
              <input className="text-input" value={name} onChange={(event) => setName(event.currentTarget.value)} />
            </label>
            <label className="field-group">
              <span className="field-label">{t("edit.source")}</span>
              <input className="text-input" value={sourceText} readOnly />
            </label>
          </div>
          <label className="field-group">
            <span className="field-label">{t("edit.description")}</span>
            <input
              className="text-input"
              value={description}
              onChange={(event) => setDescription(event.currentTarget.value)}
            />
          </label>
          <label className="field-group">
            <span className="field-label">{t("edit.location")}</span>
            <input className="text-input mono" value={tildePath(skill.sourcePath, home)} readOnly />
          </label>

          {broken ? <p className="warning-line error">{t("edit.brokenWarning")}</p> : null}
          {missingFields.length > 0 ? (
            <p className="warning-line">{t("edit.missingFields", { fields: missingFields.join(" / ") })}</p>
          ) : null}
          {nameMissing ? <p className="warning-line error">{t("edit.nameRequired")}</p> : null}

          <div className="field-group">
            <span className="field-label">{t("edit.files")}</span>
            <span className="file-chips">
              {skill.supportFiles.map((file) => (
                <span className="file-chip" key={file}>
                  {file}
                </span>
              ))}
            </span>
          </div>
        </form>
      </div>

      <footer className="page-footer">
        {confirmingRemove ? (
          <span className="confirm-group">
            <button type="button" className="button danger" disabled={busy} onClick={() => onRemove(skill)}>
              {t("edit.removeConfirm")}
            </button>
            <button type="button" className="button quiet" onClick={() => setConfirmingRemove(false)}>
              {t("actions.cancel")}
            </button>
          </span>
        ) : (
          <button type="button" className="button text-danger" disabled={busy} onClick={() => setConfirmingRemove(true)}>
            {t("edit.remove")}
          </button>
        )}
        <button type="button" className="button" onClick={() => onExport(skill)}>
          {t("edit.export")}
        </button>
        <span className="spacer" />
        <button type="button" className="button ghost" onClick={onCancel}>
          {t("actions.cancel")}
        </button>
        <button type="button" className="button primary" disabled={!dirty || nameMissing || saving || busy} onClick={save}>
          <Check size={15} strokeWidth={2.4} aria-hidden="true" />
          {saving ? t("edit.saving") : t("edit.save")}
        </button>
      </footer>
    </>
  );
}
