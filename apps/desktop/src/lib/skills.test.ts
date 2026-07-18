import { describe, expect, it } from "vitest";
import { filterSkills, getSkillStats, mergeSameNamedSkills, type SkillRecord } from "./skills";

const records: SkillRecord[] = [
  {
    id: "codex-style",
    name: "codex-style",
    description: "Codex UI and workflow guardrails",
    source: "Codex",
    sourcePath: "C:/Users/example/.codex/skills/codex-style",
    origin: { kind: "standalone", plugin: null },
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
    origin: { kind: "standalone", plugin: null },
    health: "warning",
    targets: [{ id: "codex", name: "Codex", enabled: false }],
    supportFiles: ["SKILL.md"],
  },
];

const pluginRecord: SkillRecord = {
  ...records[0],
  id: "plugin:openai/github/gh-fix-ci",
  name: "gh-fix-ci",
  sourcePath: "C:/Users/example/.codex/plugins/cache/openai-curated-remote/github/0.1.8/skills/gh-fix-ci",
  origin: {
    kind: "plugin",
    plugin: {
      id: "openai/github",
      name: "GitHub",
      version: "0.1.8",
      publisher: "OpenAI",
    },
  },
};

describe("filterSkills", () => {
  it("matches name, description, source, and path without case sensitivity", () => {
    expect(filterSkills(records, "WORKFLOW")).toHaveLength(1);
    expect(filterSkills(records, "shared")).toHaveLength(1);
    expect(filterSkills(records, ".codex")).toHaveLength(1);
    expect(filterSkills([...records, pluginRecord], "github")).toEqual([pluginRecord]);
    expect(filterSkills([...records, pluginRecord], "0.1.8")).toEqual([pluginRecord]);
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

describe("mergeSameNamedSkills", () => {
  it("merges same-named records when they describe the same skill", () => {
    const sharedRecord: SkillRecord = {
      ...records[0],
      id: "codex-style-shared",
      source: "Shared Library",
      sourcePath: "C:/Users/example/.skills-manage/library/codex-style",
      targets: records[0].targets.map((target) => ({ ...target, enabled: false })),
    };

    const merged = mergeSameNamedSkills([records[0], sharedRecord]);

    expect(merged).toHaveLength(1);
    expect(merged[0].name).toBe("codex-style");
    expect(merged[0].source).toBe("Codex, Shared Library");
    expect(merged[0].sourcePath).toBe(sharedRecord.sourcePath);
    expect(merged[0].targets.find((target) => target.id === "codex")?.enabled).toBe(true);
  });

  it("keeps plugin skills separate from standalone copies", () => {
    const sameNamedPlugin = {
      ...pluginRecord,
      name: records[0].name,
      description: records[0].description,
    };

    const merged = mergeSameNamedSkills([records[0], sameNamedPlugin]);

    expect(merged).toHaveLength(2);
    expect(merged.map((record) => record.origin.kind)).toEqual(["standalone", "plugin"]);
  });

  it("keeps same-named records separate when descriptions show different skills", () => {
    const unrelatedRecord: SkillRecord = {
      ...records[0],
      id: "codex-style-unrelated",
      description: "A different skill that happens to share the folder name.",
      source: "Claude Code",
      sourcePath: "C:/Users/example/.claude/skills/codex-style",
    };

    const merged = mergeSameNamedSkills([records[0], unrelatedRecord]);

    expect(merged).toHaveLength(2);
    expect(merged.map((record) => record.description)).toEqual([
      records[0].description,
      unrelatedRecord.description,
    ]);
  });
});
