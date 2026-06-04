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
  Gauge,
  PackageOpen,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import "./App.css";
import { demoSkills } from "./data/demoSkills";
import { filterSkills, getSkillStats, type SkillHealth, type SkillRecord } from "./lib/skills";

const healthLabels: Record<SkillHealth, string> = {
  healthy: "Healthy",
  warning: "Needs review",
  broken: "Broken",
};

const healthIcons = {
  healthy: CheckCircle2,
  warning: CircleAlert,
  broken: CircleX,
};

const sections = [
  { label: "Skills", icon: Boxes, active: true },
  { label: "Import", icon: FolderInput, active: false },
  { label: "Packages", icon: PackageOpen, active: false },
  { label: "Settings", icon: Settings, active: false },
];

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

const toggleableTargetIds = new Set(["codex", "claude-code"]);

function App() {
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
      setScanError(error instanceof Error ? error.message : "Unable to scan local folders.");
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
    setTargetActionMessage(`${nextEnabled ? "Enabling" : "Disabling"} ${selectedSkill.name} for ${target.name}.`);

    try {
      const result = await invoke<TargetToggleResult>("set_skill_target_enabled", {
        sourcePath: selectedSkill.sourcePath,
        targetId: target.id,
        enabled: nextEnabled,
      });
      setTargetActionState("success");
      setTargetActionMessage(
        result.enabled
          ? `Enabled ${result.skillName} for ${result.targetName}.`
          : `Disabled ${result.skillName} for ${result.targetName}.`,
      );
      await loadSkills();
    } catch (error) {
      setTargetActionState("error");
      setTargetActionMessage(
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : `Could not update ${target.name}. Check the target folder and try again.`,
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
    setImportMessage(`Importing ${selectedSkill.name} into the shared library.`);

    try {
      const result = await invoke<ImportResult>("import_skill_to_library", {
        sourcePath: selectedSkill.sourcePath,
      });
      setImportState("success");
      setImportMessage(
        result.imported
          ? `Imported ${result.skillName} into the shared library.`
          : result.message || `${result.skillName} is already in the shared library.`,
      );
      await loadSkills();
    } catch (error) {
      setImportState("error");
      setImportMessage(
        error instanceof Error
          ? error.message
          : typeof error === "string"
            ? error
            : "Import failed. Check the skill folder and try again.",
      );
    }
  }

  useEffect(() => {
    void loadSkills();
  }, []);

  return (
    <main className="app-shell">
      <aside className="rail" aria-label="Primary navigation">
        <div className="brand-mark" aria-hidden="true">
          SM
        </div>
        <nav className="rail-nav">
          {sections.map((section) => {
            const Icon = section.icon;
            return (
              <button
                className={`rail-button ${section.active ? "active" : ""}`}
                type="button"
                key={section.label}
                aria-current={section.active ? "page" : undefined}
                aria-label={section.label}
              >
                <Icon size={19} strokeWidth={1.8} />
                <span>{section.label}</span>
              </button>
            );
          })}
        </nav>
        <button className="rail-icon-button" type="button" aria-label="Workflow settings" disabled>
          <SlidersHorizontal size={19} strokeWidth={1.8} />
        </button>
      </aside>

      <section className="library-pane" aria-label="Skills library">
        <header className="pane-header">
          <div>
            <p className="eyebrow">Local skills library</p>
            <h1>Skills Manage</h1>
          </div>
          <span className="mode-pill">
            <ShieldCheck size={16} strokeWidth={1.8} />
            Preview safe mode
          </span>
        </header>

        <div className="stat-grid" aria-label="Library summary">
          <SummaryStat label="Skills" value={stats.total} />
          <SummaryStat label="Healthy" value={stats.healthy} />
          <SummaryStat label="Review" value={stats.warnings} />
          <SummaryStat label="Enabled links" value={stats.enabledTargets} />
        </div>

        <div className="toolbar">
          <label className="search-field">
            <Search size={17} strokeWidth={1.8} aria-hidden="true" />
            <span className="sr-only">Search skills</span>
            <input
              aria-label="Search skills"
              type="search"
              placeholder="Search skills, sources, paths"
              value={query}
              onChange={(event) => setQuery(event.currentTarget.value)}
            />
          </label>
          <button
            className="icon-action"
            type="button"
            aria-label="Scan local skills"
            disabled={scanState === "scanning"}
            onClick={() => void loadSkills()}
          >
            <RefreshCw size={18} strokeWidth={1.8} />
          </button>
        </div>

        <div className={`scan-status ${scanState === "error" ? "error" : ""}`} role="status">
          {scanState === "scanning" ? (
            <span>Scanning local folders</span>
          ) : scanState === "error" ? (
            <span>Scan unavailable: {scanError}</span>
          ) : usingFallback ? (
            <span>No local skills found. Showing sample records.</span>
          ) : (
            <span>Scan complete. Showing local skills.</span>
          )}
        </div>

        <div className="skills-list" aria-label="Discovered skills">
          {filteredSkills.map((skill) => (
            <SkillListItem
              key={skill.id}
              skill={skill}
              selected={skill.id === selectedSkill?.id}
              onSelect={() => setSelectedId(skill.id)}
            />
          ))}
          {filteredSkills.length === 0 ? (
            <div className="empty-state">
              <p>No skills match this search.</p>
              <span>Try a tool name, folder, or support file.</span>
            </div>
          ) : null}
        </div>
      </section>

      <section className="detail-pane" aria-label="Skill detail">
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
          />
        ) : null}
      </section>

      <footer className="status-strip" aria-label="App status">
        <span>Data root: %USERPROFILE%\.skills-manage</span>
        <span>Backup mode: before every managed write</span>
        <span>Package format: .skillpack</span>
      </footer>
    </main>
  );
}

function SummaryStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="summary-stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function SkillListItem({
  skill,
  selected,
  onSelect,
}: {
  skill: SkillRecord;
  selected: boolean;
  onSelect: () => void;
}) {
  const Icon = healthIcons[skill.health];
  const enabledCount = skill.targets.filter((target) => target.enabled).length;

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
        <span>{enabledCount} targets</span>
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
}) {
  const Icon = healthIcons[skill.health];
  const importLabel =
    skill.source === "Shared Library" ? "Already in library" : importState === "importing" ? "Importing" : "Import to library";

  return (
    <div className="detail-content">
      <header className="detail-header">
        <div>
          <p className="eyebrow">Selected skill</p>
          <h2>{skill.name}</h2>
        </div>
        <span className={`health-badge ${skill.health}`}>
          <Icon size={16} strokeWidth={1.9} />
          {healthLabels[skill.health]}
        </span>
      </header>

      <p className="detail-description">{skill.description}</p>

      <section className="detail-section" aria-labelledby="metadata-heading">
        <div className="section-title-row">
          <h3 id="metadata-heading">Metadata</h3>
          <Gauge size={17} strokeWidth={1.8} aria-hidden="true" />
        </div>
        <dl className="metadata-grid">
          <div>
            <dt>Source</dt>
            <dd>{skill.source}</dd>
          </div>
          <div>
            <dt>Path</dt>
            <dd>{skill.sourcePath}</dd>
          </div>
          <div>
            <dt>Support files</dt>
            <dd>{skill.supportFiles.join(", ")}</dd>
          </div>
        </dl>
      </section>

      <section className="detail-section" aria-labelledby="targets-heading">
        <div className="section-title-row">
          <h3 id="targets-heading">Target tools</h3>
          <ShieldCheck size={17} strokeWidth={1.8} aria-hidden="true" />
        </div>
        <div className="target-list">
          {skill.targets.map((target) => {
            const canToggle = skill.source === "Shared Library" && toggleableTargetIds.has(target.id) && !targetToggleLocked;
            const actionVerb = target.enabled ? "Disable" : "Enable";
            const actionLabel = `${actionVerb} ${target.name}`;
            const savingThisTarget = targetActionState === "saving" && targetActionTargetId === target.id;

            return (
              <div className="target-row" key={target.id}>
                <span>
                  <strong>{target.name}</strong>
                  <small>{target.enabled ? "Managed copy is active" : "Not enabled for this tool"}</small>
                </span>
                <button
                  type="button"
                  disabled={!canToggle}
                  aria-label={actionLabel}
                  onClick={() => onToggleTarget(target)}
                >
                  {savingThisTarget ? "Saving" : actionVerb}
                </button>
              </div>
            );
          })}
        </div>
      </section>

      <section className="action-bar" aria-label="Skill actions">
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
          Repair
        </button>
        <button type="button" disabled>
          <Download size={17} strokeWidth={1.8} />
          Export .skillpack
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
