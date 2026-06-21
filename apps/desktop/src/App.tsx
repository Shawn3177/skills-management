import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, Boxes, Download, FolderInput, PackageOpen, RefreshCw, Search, Settings } from "lucide-react";
import "./App.css";
import { LanguageSwitch } from "./components/LanguageSwitch";
import {
  hasSource,
  healthIcons,
  SkillDetail,
  SkillListItem,
  StatusMessage,
  ToggleSwitch,
  toggleableTargetIds,
  toggleableTargets,
  WorkspacePanel,
  type ImportState,
  type TargetActionState,
} from "./components/skill-ui";
import { demoSkills } from "./data/demoSkills";
import { useLocale, type TFunction } from "./i18n/useLocale";
import {
  filterSkills,
  getSkillStats,
  getSkillUsageLabels,
  mergeSameNamedSkills,
  type SkillRecord,
} from "./lib/skills";
import type { MessageKey } from "./i18n/messages";

type SectionId = "skills" | "import" | "packages" | "settings";

const sections = [
  { id: "skills", key: "nav.skills", icon: Boxes },
  { id: "import", key: "nav.imported", icon: FolderInput },
  { id: "packages", key: "nav.packages", icon: PackageOpen },
  { id: "settings", key: "nav.settings", icon: Settings },
] satisfies Array<{ id: SectionId; key: MessageKey; icon: typeof Boxes }>;

type ScanState = "scanning" | "ready" | "preview" | "error";

type ImportResult = {
  imported: boolean;
  alreadyManaged: boolean;
  skillName: string;
  libraryPath: string;
  message: string;
};

type TargetToggleResult = {
  targetId: string;
  targetName: string;
  skillName: string;
  enabled: boolean;
  changed: boolean;
  targetPath: string;
  message: string;
};

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
    setTargetActionMessage(
      t(enabled ? "actions.enablingAllForTarget" : "actions.disablingAllForTarget", { targetName }),
    );

    try {
      const result = await invoke<{ succeeded: number; failed: number }>("set_skill_targets_bulk", {
        sourcePaths: candidates.map((skill) => skill.sourcePath),
        targetId,
        enabled,
      });
      const { succeeded, failed } = result;
      setTargetActionState(failed > 0 ? "error" : "success");
      setTargetActionMessage(
        failed > 0
          ? t(enabled ? "actions.enabledAllForTargetPartial" : "actions.disabledAllForTargetPartial", {
              count: succeeded,
              failed,
              targetName,
            })
          : t(enabled ? "actions.enabledAllForTarget" : "actions.disabledAllForTarget", {
              count: succeeded,
              targetName,
            }),
      );
    } catch (error) {
      console.error("set_skill_targets_bulk failed", error);
      setTargetActionState("error");
      setTargetActionMessage(describeTargetError(error, t, targetName));
    } finally {
      setSavingKey("");
    }

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

  async function exportSkills(skills: SkillRecord[], defaultName: string) {
    if (skills.length === 0) {
      setImportMessage("");
      setUtilityMessage(t("actions.exportLibraryEmpty"));
      return;
    }

    let destination: string | null = null;
    try {
      destination = await save({
        defaultPath: defaultName,
        filters: [{ name: "Skillpack", extensions: ["skillpack"] }],
      });
    } catch (error) {
      console.error("save dialog failed", error);
    }
    if (!destination) {
      return;
    }

    setImportMessage("");
    setUtilityMessage(t("actions.exportingSkillpack"));
    try {
      const result = await invoke<{ skillCount: number }>("export_skillpack", {
        sources: skills.map((skill) => ({
          sourcePath: skill.sourcePath,
          name: skill.name,
          targets: skill.targets.map((target) => ({ id: target.id, enabled: target.enabled })),
        })),
        destination,
      });
      setUtilityMessage(t("actions.exportedSkillpack", { count: result.skillCount }));
    } catch (error) {
      console.error("export_skillpack failed", error);
      setUtilityMessage(
        isTauriBridgeUnavailable(error) ? t("errors.bridgeAction") : t("errors.exportFallback"),
      );
    }
  }

  function exportSelectedSkill() {
    if (!selectedSkill) {
      return;
    }
    void exportSkills([selectedSkill], `${selectedSkill.name}.skillpack`);
  }

  function exportLibrary() {
    void exportSkills(
      groupedSkills.filter((skill) => hasSource(skill, "Shared Library")),
      "skills-library.skillpack",
    );
  }

  async function importSkillpack() {
    let selection: string | string[] | null = null;
    try {
      selection = await open({
        multiple: false,
        filters: [{ name: "Skillpack", extensions: ["skillpack"] }],
      });
    } catch (error) {
      console.error("open dialog failed", error);
    }
    const packagePath = Array.isArray(selection) ? selection[0] : selection;
    if (!packagePath) {
      return;
    }

    setImportMessage("");
    setUtilityMessage(t("actions.importingSkillpack"));
    try {
      const result = await invoke<{ imported: number }>("import_skillpack", { packagePath });
      setUtilityMessage(t("actions.importedSkillpack", { count: result.imported }));
      await loadSkills();
    } catch (error) {
      console.error("import_skillpack failed", error);
      setUtilityMessage(
        isTauriBridgeUnavailable(error) ? t("errors.bridgeAction") : t("errors.importPackFallback"),
      );
    }
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
                onExport={exportSelectedSkill}
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
              <button type="button" onClick={exportLibrary}>
                <Download size={17} strokeWidth={1.8} />
                {t("actions.exportSkillpack")}
              </button>
              <button type="button" onClick={() => void importSkillpack()}>
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

export default App;
