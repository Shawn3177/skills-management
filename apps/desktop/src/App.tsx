import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Archive,
  ArrowLeft,
  Boxes,
  CheckCircle2,
  CircleAlert,
  CircleX,
  Download,
  FolderInput,
  PackageOpen,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
} from "lucide-react";
import "./App.css";
import { LanguageSwitch } from "./components/LanguageSwitch";
import { demoSkills } from "./data/demoSkills";
import { useLocale } from "./i18n/useLocale";
import {
  filterSkills,
  getSkillStats,
  getSkillUsageLabels,
  mergeSameNamedSkills,
  type SkillHealth,
  type SkillRecord,
} from "./lib/skills";
import type { MessageKey } from "./i18n/messages";

const healthLabelKeys: Record<SkillHealth, MessageKey> = {
  healthy: "health.healthy",
  warning: "health.warning",
  broken: "health.broken",
};

const healthIcons = {
  healthy: CheckCircle2,
  warning: CircleAlert,
  broken: CircleX,
};

type SectionId = "skills" | "import" | "packages" | "settings";

const sections = [
  { id: "skills", key: "nav.skills", icon: Boxes },
  { id: "import", key: "nav.imported", icon: FolderInput },
  { id: "packages", key: "nav.packages", icon: PackageOpen },
  { id: "settings", key: "nav.settings", icon: Settings },
] satisfies Array<{ id: SectionId; key: MessageKey; icon: typeof Boxes }>;

type ScanState = "scanning" | "ready" | "preview" | "error";
type ImportState = "idle" | "importing" | "success" | "error";

type ImportResult = {
  imported: boolean;
  alreadyManaged: boolean;
  skillName: string;
  libraryPath: string;
  message: string;
};

type TargetActionState = "idle" | "saving" | "success" | "error";

type TargetToggleResult = {
  targetId: string;
  targetName: string;
  skillName: string;
  enabled: boolean;
  changed: boolean;
  targetPath: string;
  message: string;
};

type TFunction = ReturnType<typeof useLocale>["t"];

const toggleableTargets = [
  { id: "codex", name: "Codex" },
  { id: "claude-code", name: "Claude Code" },
];
const toggleableTargetIds = new Set(toggleableTargets.map((target) => target.id));

function isTauriBridgeUnavailable(error: unknown) {
  const message = (error instanceof Error ? error.message : String(error ?? "")).toLowerCase();

  return message.includes("invoke") || message.includes("tauri") || message.includes("internals");
}

function describeImportError(error: unknown, t: TFunction) {
  return isTauriBridgeUnavailable(error) ? t("errors.bridgeAction") : t("errors.importFallback");
}

function describeTargetError(error: unknown, t: TFunction, targetName: string) {
  if (isTauriBridgeUnavailable(error)) {
    return t("errors.bridgeAction");
  }

  const raw = (error instanceof Error ? error.message : String(error ?? "")).toLowerCase();
  if (raw.includes("not managed")) {
    return t("errors.targetConflict");
  }

  return t("errors.targetFallback", { targetName });
}

function hasSource(skill: SkillRecord, source: string) {
  return skill.source
    .split(",")
    .map((entry) => entry.trim())
    .includes(source);
}

function App() {
  const { locale, setLocale, t } = useLocale();
  const [activeSection, setActiveSection] = useState<SectionId>("skills");
  const [query, setQuery] = useState("");
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [detailOpen, setDetailOpen] = useState(false);
  const [scanState, setScanState] = useState<ScanState>("scanning");
  const [importState, setImportState] = useState<ImportState>("idle");
  const [importMessage, setImportMessage] = useState("");
  const [targetActionState, setTargetActionState] = useState<TargetActionState>("idle");
  const [targetActionMessage, setTargetActionMessage] = useState("");
  const [savingKey, setSavingKey] = useState("");
  const [utilityMessage, setUtilityMessage] = useState("");
  const backButtonRef = useRef<HTMLButtonElement>(null);
  const skillListRef = useRef<HTMLElement>(null);
  const pendingFocusRef = useRef<"detail" | "list" | null>(null);
  const visibleSkills = skills.length > 0 ? skills : demoSkills;
  const groupedSkills = useMemo(() => mergeSameNamedSkills(visibleSkills), [visibleSkills]);
  const allImportCandidates = useMemo(
    () => mergeSameNamedSkills(skills).filter((skill) => !hasSource(skill, "Shared Library")),
    [skills],
  );
  const usingFallback = scanState === "ready" && skills.length === 0;
  const filteredSkills = useMemo(() => filterSkills(groupedSkills, query), [groupedSkills, query]);
  const selectedSkill = filteredSkills.find((skill) => skill.id === selectedId) ?? filteredSkills[0];
  const importedSkills = filteredSkills.filter((skill) => hasSource(skill, "Shared Library"));
  const stats = useMemo(() => getSkillStats(groupedSkills), [groupedSkills]);
  const importDisabled =
    !selectedSkill ||
    hasSource(selectedSkill, "Shared Library") ||
    scanState === "scanning" ||
    importState === "importing" ||
    targetActionState === "saving";
  const importAllDisabled =
    scanState !== "ready" ||
    importState === "importing" ||
    targetActionState === "saving" ||
    allImportCandidates.length === 0;
  const targetToggleLocked = scanState === "scanning" || importState === "importing" || targetActionState === "saving";
  const statusMessage = activeSection === "skills" && !detailOpen && importMessage ? importMessage : "";
  const scanStatusIsError = scanState === "error" || (Boolean(statusMessage) && importState === "error");
  const compactSummary = t("status.readyCompact", {
    total: stats.total,
    healthy: stats.healthy,
    warnings: stats.warnings,
  });

  async function loadSkills({
    keepDetailOpen = false,
    preserveImportMessage = false,
  }: { keepDetailOpen?: boolean; preserveImportMessage?: boolean } = {}) {
    setScanState("scanning");
    if (!preserveImportMessage) {
      setImportMessage("");
      setImportState("idle");
    }
    if (!keepDetailOpen) {
      setDetailOpen(false);
    }

    try {
      const scannedSkills = await invoke<SkillRecord[]>("scan_skills");
      const nextSkills = mergeSameNamedSkills(scannedSkills);
      setSkills(scannedSkills);
      setSelectedId((currentId) =>
        keepDetailOpen && nextSkills.some((skill) => skill.id === currentId)
          ? currentId
          : nextSkills[0]?.id ?? demoSkills[0]?.id ?? "",
      );
      setScanState("ready");
    } catch (error) {
      console.error("scan_skills failed", error);
      setSkills([]);
      setSelectedId(demoSkills[0]?.id ?? "");
      setScanState(isTauriBridgeUnavailable(error) ? "preview" : "error");
    }
  }

  async function toggleSkillTarget(
    skill: SkillRecord | undefined,
    target: { id: string; name: string; enabled: boolean },
  ) {
    if (!skill || !hasSource(skill, "Shared Library") || !toggleableTargetIds.has(target.id)) {
      return;
    }

    const nextEnabled = !target.enabled;
    setTargetActionState("saving");
    setSavingKey(`${skill.id}::${target.id}`);
    setTargetActionMessage(
      t(nextEnabled ? "actions.enablingSkillForTarget" : "actions.disablingSkillForTarget", {
        skillName: skill.name,
        targetName: target.name,
      }),
    );

    try {
      const result = await invoke<TargetToggleResult>("set_skill_target_enabled", {
        sourcePath: skill.sourcePath,
        targetId: target.id,
        enabled: nextEnabled,
      });
      setTargetActionState("success");
      setTargetActionMessage(
        t(result.enabled ? "actions.enabledSkillForTarget" : "actions.disabledSkillForTarget", {
          skillName: result.skillName,
          targetName: result.targetName,
        }),
      );
      await loadSkills({ keepDetailOpen: detailOpen, preserveImportMessage: true });
    } catch (error) {
      console.error("set_skill_target_enabled failed", error);
      setTargetActionState("error");
      setTargetActionMessage(describeTargetError(error, t, target.name));
    } finally {
      setSavingKey("");
    }
  }

  async function bulkSetTarget(targetId: string, targetName: string, enabled: boolean) {
    if (!toggleableTargetIds.has(targetId) || targetToggleLocked) {
      return;
    }

    const candidates = importedSkills.filter((skill) => {
      const target = skill.targets.find((entry) => entry.id === targetId);
      return target !== undefined && hasSource(skill, "Shared Library") && target.enabled !== enabled;
    });

    if (candidates.length === 0) {
      setTargetActionState("success");
      setTargetActionMessage(t("actions.bulkTargetNoChange", { targetName }));
      return;
    }

    setTargetActionState("saving");
    setSavingKey(`bulk::${targetId}::${enabled ? "on" : "off"}`);

    let completed = 0;
    let lastError: unknown = null;

    for (const skill of candidates) {
      setTargetActionMessage(
        t(enabled ? "actions.enablingSkillForTarget" : "actions.disablingSkillForTarget", {
          skillName: skill.name,
          targetName,
        }),
      );

      try {
        await invoke<TargetToggleResult>("set_skill_target_enabled", {
          sourcePath: skill.sourcePath,
          targetId,
          enabled,
        });
        completed += 1;
      } catch (error) {
        console.error("set_skill_target_enabled failed", error);
        lastError = error;
      }
    }

    const failed = candidates.length - completed;

    if (completed > 0) {
      setTargetActionState(failed > 0 ? "error" : "success");
      setTargetActionMessage(
        failed > 0
          ? t(enabled ? "actions.enabledAllForTargetPartial" : "actions.disabledAllForTargetPartial", {
              count: completed,
              failed,
              targetName,
            })
          : t(enabled ? "actions.enabledAllForTarget" : "actions.disabledAllForTarget", {
              count: completed,
              targetName,
            }),
      );
    } else {
      setTargetActionState("error");
      setTargetActionMessage(describeTargetError(lastError, t, targetName));
    }

    setSavingKey("");
    await loadSkills({ keepDetailOpen: detailOpen, preserveImportMessage: true });
  }

  async function importSkillToLibrary(skill: SkillRecord) {
    return invoke<ImportResult>("import_skill_to_library", {
      sourcePath: skill.sourcePath,
    });
  }

  async function importSkill(skill: SkillRecord | undefined) {
    if (!skill || hasSource(skill, "Shared Library") || scanState === "scanning" || importState === "importing") {
      return;
    }

    setImportState("importing");
    setUtilityMessage("");
    setImportMessage(t("actions.importingSkill", { skillName: skill.name }));

    try {
      const result = await importSkillToLibrary(skill);
      setImportState("success");
      setImportMessage(
        result.imported
          ? t("actions.importedSkill", { skillName: result.skillName })
          : result.alreadyManaged
            ? t("actions.skillAlreadyManaged", { skillName: result.skillName })
            : result.message || t("actions.skillAlreadyManaged", { skillName: result.skillName }),
      );
      await loadSkills({ keepDetailOpen: true, preserveImportMessage: true });
    } catch (error) {
      console.error("import_skill_to_library failed", error);
      setImportState("error");
      setImportMessage(describeImportError(error, t));
    }
  }

  async function importSelectedSkill() {
    await importSkill(selectedSkill);
  }

  async function importAllSkills() {
    if (importAllDisabled) {
      if (allImportCandidates.length === 0 && scanState === "ready") {
        setImportState("success");
        setImportMessage(t("actions.importAllEmpty"));
      }
      return;
    }

    const candidates = allImportCandidates;
    setDetailOpen(false);
    setImportState("importing");
    setUtilityMessage("");
    setTargetActionMessage("");

    let completedCount = 0;
    let lastError: unknown = null;

    for (const [index, skill] of candidates.entries()) {
      setImportMessage(
        t("actions.importingAllSkills", {
          current: index + 1,
          total: candidates.length,
          skillName: skill.name,
        }),
      );

      try {
        const result = await importSkillToLibrary(skill);

        if (result.imported || result.alreadyManaged) {
          completedCount += 1;
        }
      } catch (error) {
        console.error("import_skill_to_library failed", error);
        lastError = error;
      }
    }

    const failedCount = candidates.length - completedCount;

    if (completedCount > 0) {
      setImportState(failedCount > 0 ? "error" : "success");
      setImportMessage(
        failedCount > 0
          ? t("actions.importedAllSkillsPartial", { count: completedCount, failed: failedCount })
          : t("actions.importedAllSkills", { count: completedCount }),
      );
    } else {
      setImportState("error");
      setImportMessage(describeImportError(lastError, t));
    }

    await loadSkills({ preserveImportMessage: true });
  }

  function selectSection(sectionId: SectionId) {
    setActiveSection(sectionId);
    setDetailOpen(false);
    setUtilityMessage("");
  }

  function openSkillDetail(skillId: string) {
    pendingFocusRef.current = "detail";
    setSelectedId(skillId);
    setDetailOpen(true);
    setImportMessage("");
    setTargetActionMessage("");
    setUtilityMessage("");
  }

  function closeSkillDetail() {
    pendingFocusRef.current = "list";
    setDetailOpen(false);
    setImportMessage("");
    setTargetActionMessage("");
    setUtilityMessage("");
  }

  function showRepairPreview() {
    if (!selectedSkill) {
      return;
    }

    setImportMessage("");
    setUtilityMessage(t("actions.repairPreview", { skillName: selectedSkill.name }));
  }

  function showExportPreview() {
    if (!selectedSkill) {
      return;
    }

    setImportMessage("");
    setUtilityMessage(t("actions.exportPreview", { skillName: selectedSkill.name }));
  }

  function showTargetBlocked(target: { id: string; name: string }) {
    if (!selectedSkill) {
      return;
    }

    setImportMessage("");
    setUtilityMessage("");
    setTargetActionState("error");
    setSavingKey("");
    setTargetActionMessage(
      t(hasSource(selectedSkill, "Shared Library") ? "targets.pendingAction" : "targets.importRequiredAction", {
        skillName: selectedSkill.name,
        targetName: target.name,
      }),
    );
  }

  useEffect(() => {
    void loadSkills();
  }, []);

  useEffect(() => {
    const pending = pendingFocusRef.current;
    if (!pending) {
      return;
    }
    pendingFocusRef.current = null;

    if (pending === "detail") {
      backButtonRef.current?.focus();
      return;
    }

    const list = skillListRef.current;
    if (!list) {
      return;
    }
    const row = list.querySelector<HTMLElement>(`[data-skill-id="${selectedId}"]`);
    (row ?? list).focus();
  }, [detailOpen, selectedId]);

  return (
    <main className="app-shell">
      <aside className="app-rail" aria-label={t("regions.appControls")}>
        <div className="rail-brand" aria-label={t("app.title")}>
          <Boxes size={20} strokeWidth={1.9} aria-hidden="true" />
          <span>SM</span>
        </div>

        <nav className="rail-nav" aria-label={t("regions.primaryNavigation")}>
          {sections.map((section) => {
            const Icon = section.icon;
            const label = t(section.key);
            const active = activeSection === section.id;
            return (
              <button
                className={`rail-button ${active ? "active" : ""}`}
                type="button"
                key={section.key}
                aria-current={active ? "page" : undefined}
                aria-label={label}
                title={label}
                onClick={() => selectSection(section.id)}
              >
                <Icon size={18} strokeWidth={1.8} />
              </button>
            );
          })}
        </nav>

        <div className="rail-spacer" aria-hidden="true" />
      </aside>

      <section className="skills-workspace" aria-label={t("regions.skillsLibrary")}>
        <header className="command-bar">
          <div className="command-title">
            <strong>{t("app.title")}</strong>
            <span>{compactSummary}</span>
          </div>

          <label className="search-field">
            <Search size={17} strokeWidth={1.8} aria-hidden="true" />
            <span className="sr-only">{t("search.label")}</span>
            <input
              aria-label={t("search.label")}
              type="search"
              placeholder={t("search.placeholder")}
              value={query}
              onChange={(event) => {
                setQuery(event.currentTarget.value);
                setDetailOpen(false);
              }}
            />
          </label>

          {activeSection === "skills" ? (
            <button
              className="bulk-import-action"
              type="button"
              disabled={importAllDisabled}
              onClick={() => void importAllSkills()}
            >
              <FolderInput size={17} strokeWidth={1.8} />
              {importState === "importing" ? t("actions.importing") : t("actions.importAllToLibrary")}
            </button>
          ) : (
            <span aria-hidden="true" />
          )}

          <button
            className="icon-action"
            type="button"
            aria-label={t("actions.scan")}
            disabled={scanState === "scanning"}
            onClick={() => void loadSkills()}
          >
            <RefreshCw size={18} strokeWidth={1.8} />
          </button>

          <LanguageSwitch label={t("language.label")} locale={locale} onLocaleChange={setLocale} />
        </header>

        <div className={`scan-status ${scanStatusIsError ? "error" : ""}`} role={scanStatusIsError ? "alert" : "status"}>
          {statusMessage ? (
            <span>{statusMessage}</span>
          ) : scanState === "scanning" ? (
            <span>{t("status.scan.scanning")}</span>
          ) : scanState === "error" ? (
            <span>{t("status.scan.error")}</span>
          ) : scanState === "preview" ? (
            <span>{t("status.scan.browserPreview")}</span>
          ) : usingFallback ? (
            <span>{t("status.scan.fallback")}</span>
          ) : (
            <span>{t("status.scan.ready")}</span>
          )}
        </div>

        {activeSection === "skills" ? (
          detailOpen && selectedSkill ? (
            <section className="skill-detail-view" aria-label={t("regions.skillDetail")}>
              <div className="detail-view-toolbar">
                <button
                  className="back-action"
                  type="button"
                  ref={backButtonRef}
                  aria-label={t("actions.backToSkills")}
                  onClick={closeSkillDetail}
                >
                  <ArrowLeft size={17} strokeWidth={1.9} />
                  {t("actions.backToSkills")}
                </button>
              </div>
              <SkillDetail
                skill={selectedSkill}
                importDisabled={importDisabled}
                importMessage={importMessage}
                importState={importState}
                onExport={showExportPreview}
                onImport={() => void importSelectedSkill()}
                onRepair={showRepairPreview}
                onTargetBlocked={showTargetBlocked}
                onToggleTarget={(target) => void toggleSkillTarget(selectedSkill, target)}
                targetActionMessage={targetActionMessage}
                targetActionState={targetActionState}
                savingKey={savingKey}
                targetToggleLocked={targetToggleLocked}
                utilityMessage={utilityMessage}
                t={t}
              />
            </section>
          ) : (
            <section
              className="skills-list scroll-surface"
              aria-label={t("regions.discoveredSkills")}
              ref={skillListRef}
              tabIndex={-1}
            >
              {filteredSkills.map((skill) => (
                <SkillListItem
                  key={skill.id}
                  skill={skill}
                  selected={skill.id === selectedId}
                  onSelect={() => openSkillDetail(skill.id)}
                  usageLabels={getSkillUsageLabels(skill)}
                />
              ))}
              {filteredSkills.length === 0 ? (
                <div className="empty-state">
                  <p>{t("status.empty.title")}</p>
                  <span>{t("status.empty.body")}</span>
                </div>
              ) : null}
            </section>
          )
        ) : null}

        {activeSection === "import" ? (
          <WorkspacePanel
            icon={FolderInput}
            label={t("regions.importWorkspace")}
            title={t("workspace.import.title")}
            body={t("workspace.import.body")}
          >
            {importedSkills.length > 0 ? (
              <div className="bulk-target-bar" aria-label={t("workspace.import.bulkActions")}>
                {toggleableTargets.map((tool) => {
                  const total = importedSkills.length;
                  const enabledCount = importedSkills.filter(
                    (skill) => skill.targets.find((entry) => entry.id === tool.id)?.enabled,
                  ).length;
                  return (
                    <div className="bulk-target-row" key={tool.id}>
                      <span className="bulk-target-name">{tool.name}</span>
                      <span className="bulk-target-actions">
                        <button
                          type="button"
                          disabled={targetToggleLocked || total === 0 || enabledCount === total}
                          aria-label={t("actions.enableAllForTarget", { targetName: tool.name })}
                          onClick={() => void bulkSetTarget(tool.id, tool.name, true)}
                        >
                          {t("actions.enableAll")}
                        </button>
                        <button
                          type="button"
                          disabled={targetToggleLocked || enabledCount === 0}
                          aria-label={t("actions.disableAllForTarget", { targetName: tool.name })}
                          onClick={() => void bulkSetTarget(tool.id, tool.name, false)}
                        >
                          {t("actions.disableAll")}
                        </button>
                      </span>
                    </div>
                  );
                })}
              </div>
            ) : null}
            <div className="workspace-list">
              {importedSkills.map((skill) => {
                const Health = healthIcons[skill.health];
                return (
                  <article className="imported-item" key={skill.id}>
                    <span className="imported-item-main">
                      <span className={`health-dot ${skill.health}`}>
                        <Health size={16} strokeWidth={1.9} />
                      </span>
                      <span className="imported-item-text">
                        <strong>{skill.name}</strong>
                        <small>{skill.description}</small>
                      </span>
                    </span>
                    <span className="imported-item-targets">
                      {skill.targets.map((target) => {
                        const toggleable = toggleableTargetIds.has(target.id);
                        const actionLabel = `${target.enabled ? t("actions.disable") : t("actions.enable")} ${target.name}`;
                        const busy = targetActionState === "saving" && savingKey === `${skill.id}::${target.id}`;
                        return (
                          <span className="target-switch" key={target.id}>
                            <span className="target-switch-name">{target.name}</span>
                            {toggleable ? (
                              <ToggleSwitch
                                checked={target.enabled}
                                disabled={targetToggleLocked}
                                busy={busy}
                                label={actionLabel}
                                onToggle={() => void toggleSkillTarget(skill, target)}
                              />
                            ) : (
                              <span className="target-pending">{t("targets.pending")}</span>
                            )}
                          </span>
                        );
                      })}
                    </span>
                  </article>
                );
              })}
              {importedSkills.length === 0 ? (
                <div className="empty-state">
                  <p>{t("workspace.import.empty")}</p>
                  <span>{t("workspace.import.emptyHint")}</span>
                </div>
              ) : null}
            </div>
            {targetActionMessage ? (
              <StatusMessage state={targetActionState === "error" ? "error" : "success"} message={targetActionMessage} />
            ) : null}
          </WorkspacePanel>
        ) : null}

        {activeSection === "packages" ? (
          <WorkspacePanel
            icon={PackageOpen}
            label={t("regions.packagesWorkspace")}
            title={t("workspace.packages.title")}
            body={t("workspace.packages.body")}
          >
            <div className="workspace-actions">
              <button type="button" onClick={showExportPreview}>
                <Download size={17} strokeWidth={1.8} />
                {t("actions.exportSkillpack")}
              </button>
              <button type="button" onClick={() => setUtilityMessage(t("workspace.packages.importPreview"))}>
                <FolderInput size={17} strokeWidth={1.8} />
                {t("nav.import")}
              </button>
            </div>
            {utilityMessage ? <StatusMessage state="success" message={utilityMessage} /> : null}
          </WorkspacePanel>
        ) : null}

        {activeSection === "settings" ? (
          <WorkspacePanel
            icon={Settings}
            label={t("regions.settingsWorkspace")}
            title={t("workspace.settings.title")}
            body={t("workspace.settings.body")}
          >
            <dl className="metadata-grid settings-grid" aria-label={t("regions.appStatus")}>
              <div>
                <dt>{t("footer.dataRoot")}</dt>
                <dd>%USERPROFILE%\.skills-manage</dd>
              </div>
              <div>
                <dt>{t("footer.backupMode")}</dt>
                <dd>{t("workspace.settings.backupValue")}</dd>
              </div>
              <div>
                <dt>{t("footer.packageFormat")}</dt>
                <dd>.skillpack</dd>
              </div>
            </dl>
          </WorkspacePanel>
        ) : null}
      </section>
    </main>
  );
}

function SkillListItem({
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
      <span className="skill-row-meta">
        {usageLabels.map((label) => (
          <span className="usage-pill" key={label}>
            {label}
          </span>
        ))}
      </span>
    </button>
  );
}

function ToggleSwitch({
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

function SkillDetail({
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
  const importLabel =
    hasSource(skill, "Shared Library")
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

function WorkspacePanel({
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

function StatusMessage({ message, state }: { message: string; state: "success" | "error" }) {
  return (
    <div className={`utility-status ${state}`} role={state === "error" ? "alert" : "status"}>
      {message}
    </div>
  );
}

export default App;
