import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Archive,
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
import { filterSkills, getSkillStats, type SkillHealth, type SkillRecord } from "./lib/skills";
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

const sections = [
  { key: "nav.skills", icon: Boxes, active: true },
  { key: "nav.import", icon: FolderInput, active: false },
  { key: "nav.packages", icon: PackageOpen, active: false },
  { key: "nav.settings", icon: Settings, active: false },
] satisfies Array<{ key: MessageKey; icon: typeof Boxes; active: boolean }>;

type ScanState = "scanning" | "ready" | "error";
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

const toggleableTargetIds = new Set(["codex", "claude-code"]);

function App() {
  const { locale, setLocale, t } = useLocale();
  const [query, setQuery] = useState("");
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [scanState, setScanState] = useState<ScanState>("scanning");
  const [scanError, setScanError] = useState("");
  const [importState, setImportState] = useState<ImportState>("idle");
  const [importMessage, setImportMessage] = useState("");
  const [targetActionState, setTargetActionState] = useState<TargetActionState>("idle");
  const [targetActionMessage, setTargetActionMessage] = useState("");
  const [targetActionTargetId, setTargetActionTargetId] = useState("");
  const visibleSkills = skills.length > 0 ? skills : demoSkills;
  const usingFallback = scanState === "ready" && skills.length === 0;
  const filteredSkills = useMemo(() => filterSkills(visibleSkills, query), [query, visibleSkills]);
  const selectedSkill = filteredSkills.find((skill) => skill.id === selectedId) ?? filteredSkills[0];
  const stats = useMemo(() => getSkillStats(visibleSkills), [visibleSkills]);
  const importDisabled =
    !selectedSkill ||
    selectedSkill.source === "Shared Library" ||
    scanState === "scanning" ||
    importState === "importing" ||
    targetActionState === "saving";
  const targetToggleLocked = scanState === "scanning" || importState === "importing" || targetActionState === "saving";
  const compactSummary = t("status.readyCompact", {
    total: stats.total,
    healthy: stats.healthy,
    warnings: stats.warnings,
  });

  async function loadSkills() {
    setScanState("scanning");
    setScanError("");

    try {
      const scannedSkills = await invoke<SkillRecord[]>("scan_skills");
      setSkills(scannedSkills);
      setSelectedId(scannedSkills[0]?.id ?? demoSkills[0]?.id ?? "");
      setScanState("ready");
    } catch (error) {
      setSkills([]);
      setSelectedId(demoSkills[0]?.id ?? "");
      setScanError(error instanceof Error ? error.message : t("errors.scanFallback"));
      setScanState("error");
    }
  }

  async function toggleSkillTarget(target: { id: string; name: string; enabled: boolean }) {
    if (!selectedSkill || selectedSkill.source !== "Shared Library" || !toggleableTargetIds.has(target.id)) {
      return;
    }

    const nextEnabled = !target.enabled;
    setTargetActionState("saving");
    setTargetActionTargetId(target.id);
    setTargetActionMessage(
      t(nextEnabled ? "actions.enablingSkillForTarget" : "actions.disablingSkillForTarget", {
        skillName: selectedSkill.name,
        targetName: target.name,
      }),
    );

    try {
      const result = await invoke<TargetToggleResult>("set_skill_target_enabled", {
        sourcePath: selectedSkill.sourcePath,
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
      await loadSkills();
    } catch (error) {
      setTargetActionState("error");
      setTargetActionMessage(
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : t("errors.targetFallback", { targetName: target.name }),
      );
    } finally {
      setTargetActionTargetId("");
    }
  }

  async function importSelectedSkill() {
    if (!selectedSkill || selectedSkill.source === "Shared Library") {
      return;
    }

    setImportState("importing");
    setImportMessage(t("actions.importingSkill", { skillName: selectedSkill.name }));

    try {
      const result = await invoke<ImportResult>("import_skill_to_library", {
        sourcePath: selectedSkill.sourcePath,
      });
      setImportState("success");
      setImportMessage(
        result.imported
          ? t("actions.importedSkill", { skillName: result.skillName })
          : result.alreadyManaged
            ? t("actions.skillAlreadyManaged", { skillName: result.skillName })
            : result.message || t("actions.skillAlreadyManaged", { skillName: result.skillName }),
      );
      await loadSkills();
    } catch (error) {
      setImportState("error");
      setImportMessage(
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : t("errors.importFallback"),
      );
    }
  }

  useEffect(() => {
    void loadSkills();
  }, []);

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
            return (
              <button
                className={`rail-button ${section.active ? "active" : ""}`}
                type="button"
                key={section.key}
                aria-current={section.active ? "page" : undefined}
                aria-label={label}
                title={label}
                disabled={!section.active}
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
              onChange={(event) => setQuery(event.currentTarget.value)}
            />
          </label>

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

        <div className={`scan-status ${scanState === "error" ? "error" : ""}`} role="status">
          {scanState === "scanning" ? (
            <span>{t("status.scan.scanning")}</span>
          ) : scanState === "error" ? (
            <span>{t("status.scan.error", { error: scanError })}</span>
          ) : usingFallback ? (
            <span>{t("status.scan.fallback")}</span>
          ) : (
            <span>{t("status.scan.ready")}</span>
          )}
        </div>

        <section className="skills-list scroll-surface" aria-label={t("regions.discoveredSkills")}>
          {filteredSkills.map((skill) => (
            <SkillListItem
              key={skill.id}
              skill={skill}
              selected={skill.id === selectedSkill?.id}
              onSelect={() => setSelectedId(skill.id)}
              targetCountText={t("targets.count", {
                count: skill.targets.filter((target) => target.enabled).length,
              })}
            />
          ))}
          {filteredSkills.length === 0 ? (
            <div className="empty-state">
              <p>{t("status.empty.title")}</p>
              <span>{t("status.empty.body")}</span>
            </div>
          ) : null}
        </section>

        <section className="skill-drawer" aria-label={t("regions.skillDetail")}>
          {selectedSkill ? (
            <SkillDetail
              skill={selectedSkill}
              importDisabled={importDisabled}
              importMessage={importMessage}
              importState={importState}
              onImport={() => void importSelectedSkill()}
              onToggleTarget={(target) => void toggleSkillTarget(target)}
              targetActionMessage={targetActionMessage}
              targetActionState={targetActionState}
              targetActionTargetId={targetActionTargetId}
              targetToggleLocked={targetToggleLocked}
              t={t}
            />
          ) : null}
        </section>
      </section>
    </main>
  );
}

function SkillListItem({
  skill,
  selected,
  onSelect,
  targetCountText,
}: {
  skill: SkillRecord;
  selected: boolean;
  onSelect: () => void;
  targetCountText: string;
}) {
  const Icon = healthIcons[skill.health];

  return (
    <button className={`skill-row ${selected ? "selected" : ""}`} type="button" onClick={onSelect}>
      <span className={`health-dot ${skill.health}`}>
        <Icon size={16} strokeWidth={1.9} />
      </span>
      <span className="skill-row-main">
        <strong>{skill.name}</strong>
        <span>{skill.description}</span>
      </span>
      <span className="skill-row-meta">
        <span>{skill.source}</span>
        <span>{targetCountText}</span>
      </span>
    </button>
  );
}

function SkillDetail({
  skill,
  importDisabled,
  importMessage,
  importState,
  onImport,
  onToggleTarget,
  targetActionMessage,
  targetActionState,
  targetActionTargetId,
  targetToggleLocked,
  t,
}: {
  skill: SkillRecord;
  importDisabled: boolean;
  importMessage: string;
  importState: ImportState;
  onImport: () => void;
  onToggleTarget: (target: { id: string; name: string; enabled: boolean }) => void;
  targetActionMessage: string;
  targetActionState: TargetActionState;
  targetActionTargetId: string;
  targetToggleLocked: boolean;
  t: TFunction;
}) {
  const Icon = healthIcons[skill.health];
  const importLabel =
    skill.source === "Shared Library"
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
            const canToggle = skill.source === "Shared Library" && toggleableTargetIds.has(target.id) && !targetToggleLocked;
            const actionVerb = target.enabled ? t("actions.disable") : t("actions.enable");
            const actionLabel = `${actionVerb} ${target.name}`;
            const savingThisTarget = targetActionState === "saving" && targetActionTargetId === target.id;
            const targetStatus =
              skill.source === "Shared Library"
                ? target.enabled
                  ? t("targets.enabled")
                  : t("targets.disabled")
                : t("targets.unavailable");

            return (
              <div className="target-row" key={target.id}>
                <span>
                  <strong>{target.name}</strong>
                  <small>{targetStatus}</small>
                </span>
                <button
                  type="button"
                  disabled={!canToggle}
                  aria-label={actionLabel}
                  onClick={() => onToggleTarget(target)}
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
        <button type="button" disabled>
          <Archive size={17} strokeWidth={1.8} />
          {t("actions.repair")}
        </button>
        <button type="button" disabled>
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
    </div>
  );
}

export default App;
