import type { SkillRecord } from "../lib/skills";

export const demoSkills: SkillRecord[] = [
  {
    id: "codex-workflow-guardrails",
    name: "codex-workflow-guardrails",
    description: "Keeps Codex work aligned with branch, PR, verification, and local-file rules.",
    source: "Codex",
    sourcePath: "C:/Users/Shawn/.codex/skills/codex-workflow-guardrails",
    health: "healthy",
    targets: [
      { id: "codex", name: "Codex", enabled: true },
      { id: "claude-code", name: "Claude Code", enabled: false },
      { id: "vs-code", name: "VS Code", enabled: false },
    ],
    supportFiles: ["SKILL.md", "references/workflow.md"],
  },
  {
    id: "agent-tool-safety",
    name: "agent-tool-safety",
    description: "Designs small, auditable tools with explicit permissions and safe fallbacks.",
    source: "Shared Library",
    sourcePath: "C:/Users/Shawn/.skills-manage/library/agent-tool-safety",
    health: "warning",
    targets: [
      { id: "codex", name: "Codex", enabled: true },
      { id: "claude-code", name: "Claude Code", enabled: true },
      { id: "vs-code", name: "VS Code", enabled: false },
    ],
    supportFiles: ["SKILL.md", "scripts/validate-tools.ps1"],
  },
  {
    id: "frontend-design-review",
    name: "frontend-design-review",
    description: "Reviews product UI for density, accessibility, hierarchy, and interaction states.",
    source: "Claude Code",
    sourcePath: "C:/Users/Shawn/.claude/skills/frontend-design-review",
    health: "healthy",
    targets: [
      { id: "codex", name: "Codex", enabled: false },
      { id: "claude-code", name: "Claude Code", enabled: true },
      { id: "vs-code", name: "VS Code", enabled: true },
    ],
    supportFiles: ["SKILL.md", "assets/checklist.json"],
  },
  {
    id: "stale-pack-export",
    name: "stale-pack-export",
    description: "Exports skill packs, but its original source path is missing from this machine.",
    source: "Shared Library",
    sourcePath: "C:/Users/Shawn/.skills-manage/library/stale-pack-export",
    health: "broken",
    targets: [
      { id: "codex", name: "Codex", enabled: false },
      { id: "claude-code", name: "Claude Code", enabled: false },
      { id: "vs-code", name: "VS Code", enabled: false },
    ],
    supportFiles: ["SKILL.md", "manifest.json"],
  },
];
