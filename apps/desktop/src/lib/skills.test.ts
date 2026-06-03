import { describe, expect, it } from "vitest";
import { filterSkills, getSkillStats, type SkillRecord } from "./skills";

const records: SkillRecord[] = [
  {
    id: "codex-style",
    name: "codex-style",
    description: "Codex UI and workflow guardrails",
    source: "Codex",
    sourcePath: "C:/Users/example/.codex/skills/codex-style",
    health: "healthy",
    targets: [
      { id: "codex", name: "Codex", enabled: true },
      { id: "claude", name: "Claude Code", enabled: false },
    ],
    supportFiles: ["SKILL.md", "references/design.md"],
  },
  {
    id: "agent-pack",
    name: "agent-pack",
    description: "Reusable agent tool design rules",
    source: "Shared Library",
    sourcePath: "C:/Users/example/.skills-manage/library/agent-pack",
    health: "warning",
    targets: [{ id: "codex", name: "Codex", enabled: false }],
    supportFiles: ["SKILL.md"],
  },
];

describe("filterSkills", () => {
  it("matches name, description, source, and path without case sensitivity", () => {
    expect(filterSkills(records, "WORKFLOW")).toHaveLength(1);
    expect(filterSkills(records, "shared")).toHaveLength(1);
    expect(filterSkills(records, ".codex")).toHaveLength(1);
  });

  it("returns all records for a blank query", () => {
    expect(filterSkills(records, "   ")).toEqual(records);
  });
});

describe("getSkillStats", () => {
  it("counts healthy skills, warnings, and enabled target links", () => {
    expect(getSkillStats(records)).toEqual({
      total: 2,
      healthy: 1,
      warnings: 1,
      enabledTargets: 1,
    });
  });
});
