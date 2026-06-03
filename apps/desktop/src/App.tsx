import { useMemo, useState } from "react";
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

function App() {
  const [query, setQuery] = useState("");
  const [selectedId, setSelectedId] = useState(demoSkills[0]?.id ?? "");
  const filteredSkills = useMemo(() => filterSkills(demoSkills, query), [query]);
  const selectedSkill = filteredSkills.find((skill) => skill.id === selectedId) ?? filteredSkills[0];
  const stats = useMemo(() => getSkillStats(demoSkills), []);

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
          <button className="icon-action" type="button" aria-label="Scan local skills" disabled>
            <RefreshCw size={18} strokeWidth={1.8} />
          </button>
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
        {selectedSkill ? <SkillDetail skill={selectedSkill} /> : null}
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

function SkillDetail({ skill }: { skill: SkillRecord }) {
  const Icon = healthIcons[skill.health];

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
          {skill.targets.map((target) => (
            <div className="target-row" key={target.id}>
              <span>
                <strong>{target.name}</strong>
                <small>{target.enabled ? "Managed link is active" : "Not enabled for this tool"}</small>
              </span>
              <button type="button" disabled>
                {target.enabled ? "Disable" : "Enable"}
              </button>
            </div>
          ))}
        </div>
      </section>

      <section className="action-bar" aria-label="Skill actions">
        <button type="button" disabled>
          <FolderInput size={17} strokeWidth={1.8} />
          Import folder
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
    </div>
  );
}

export default App;
