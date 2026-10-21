import { describe, expect, it } from "vitest";
import {
  enabledCount,
  filterSkills,
  folderName,
  groupBySeries,
  groupManagedByCodex,
  isManagedByCodex,
  monogram,
  originKind,
  parseGithubLocation,
  partitionSkills,
  sourceLine,
  tildePath,
  tileColors,
  type SkillRecord,
} from "./skills";

function record(overrides: Partial<SkillRecord>): SkillRecord {
  return {
    id: overrides.name ?? "skill",
    name: "skill",
    description: "",
    source: "Shared Library",
    sourcePath: "C:/Users/example/.skills-manage/library/skill",
    health: "healthy",
    targets: [
      { id: "claude-code", name: "Claude Code", enabled: false },
      { id: "codex", name: "Codex", enabled: false },
    ],
    supportFiles: ["SKILL.md"],
    ...overrides,
  };
}

const review = record({
  name: "代码审查",
  sourcePath: "C:/Users/example/.skills-manage/library/代码审查",
  sourceUrl: "https://github.com/owner/skills/tree/main/代码审查",
  targets: [
    { id: "claude-code", name: "Claude Code", enabled: true },
    { id: "codex", name: "Codex", enabled: true },
  ],
});
const safety = record({
  name: "agent-tool-safety",
  sourcePath: "C:/Users/example/.skills-manage/library/agent-tool-safety",
  targets: [
    { id: "claude-code", name: "Claude Code", enabled: true },
    { id: "codex", name: "Codex", enabled: false },
  ],
});
const managedCopy = record({
  name: "agent-tool-safety",
  source: "Claude Code",
  sourcePath: "C:/Users/example/.claude/skills/agent-tool-safety",
  managed: true,
});
const importedOriginal = record({
  name: "代码审查",
  source: "Codex",
  sourcePath: "C:\\Users\\example\\.codex\\skills\\代码审查",
});
const localOnly = record({
  name: "pr-description",
  source: "Claude Code",
  sourcePath: "C:/Users/example/.claude/skills/pr-description",
});

describe("partitionSkills", () => {
  it("separates library skills from skills that live only in a tool folder", () => {
    const { library, locals } = partitionSkills([localOnly, review, managedCopy, safety, importedOriginal]);

    expect(library.map((skill) => skill.name)).toEqual(["agent-tool-safety", "代码审查"]);
    expect(locals.map((skill) => skill.name)).toEqual(["pr-description"]);
  });

  it("counts library skills enabled per tool", () => {
    expect(enabledCount([review, safety], "claude-code")).toBe(2);
    expect(enabledCount([review, safety], "codex")).toBe(1);
  });
});

describe("filterSkills", () => {
  it("matches name, description, path and source URL case-insensitively", () => {
    expect(filterSkills([review, safety], "SAFETY")).toEqual([safety]);
    expect(filterSkills([review, safety], "owner/skills")).toEqual([review]);
    expect(filterSkills([review, safety], "   ")).toEqual([review, safety]);
  });
});

describe("display helpers", () => {
  it("describes where a library skill came from", () => {
    const home = String.raw`C:\Users\example`;
    expect(sourceLine(review, home)).toEqual({
      kind: "github",
      text: "github.com/owner/skills/tree/main/代码审查",
      url: review.sourceUrl,
    });
    expect(
      sourceLine({ ...safety, originKind: "local", originDetail: String.raw`C:\Users\example\.claude\skills\agent-tool-safety` }, home),
    ).toEqual({ kind: "local", path: "~/.claude/skills" });
    expect(sourceLine({ ...safety, originKind: "skillpack", originDetail: "team.skillpack" }, home)).toEqual({
      kind: "skillpack",
      name: "team.skillpack",
    });
    expect(sourceLine(safety, home)).toEqual({ kind: "library", folder: "agent-tool-safety" });
  });

  it("shortens paths under the home folder", () => {
    expect(tildePath(String.raw`C:\Users\Example\.codex\skills\x`, String.raw`C:\Users\example`)).toBe(
      "~/.codex/skills/x",
    );
    expect(tildePath("D:/other/x", String.raw`C:\Users\example`)).toBe("D:/other/x");
    expect(tildePath(String.raw`\\?\C:\Users\example\.agents\skills`, String.raw`C:\Users\example`)).toBe("~/.agents/skills");
  });

  it("derives folder names from either path separator", () => {
    expect(folderName("C:\\a\\b\\skill-one")).toBe("skill-one");
    expect(folderName("/home/a/skill-two/")).toBe("skill-two");
  });

  it("builds short monograms for Latin and CJK names", () => {
    expect(monogram("agent-tool-safety")).toBe("AT");
    expect(monogram("frontend")).toBe("FR");
    expect(monogram("代码审查")).toBe("代");
  });

  it("keeps tile colors stable per name", () => {
    expect(tileColors("代码审查")).toEqual(tileColors("代码审查"));
  });

  it("reads owner, repository, ref and folder from GitHub URLs", () => {
    expect(parseGithubLocation("https://github.com/anthropics/skills/tree/main/skills/pdf")).toEqual({
      owner: "anthropics",
      repo: "skills",
      ref: "main",
      path: ["skills", "pdf"],
    });
    expect(parseGithubLocation("github.com/Panniantong/Agent-Reach.git")).toEqual({
      owner: "Panniantong",
      repo: "Agent-Reach",
      ref: null,
      path: [],
    });
    expect(parseGithubLocation("https://gitlab.com/a/b")).toBeNull();
  });

  it("groups two or more skills from one repository into a series", () => {
    const pdf = record({ name: "pdf", detectedSourceUrl: "https://github.com/anthropics/skills/tree/main/skills/pdf", linkedSource: true });
    const docx = record({ name: "docx", sourceUrl: "https://github.com/Anthropics/skills/tree/main/skills/docx" });
    const reach = record({ name: "agent-reach", detectedSourceUrl: "https://github.com/Panniantong/Agent-Reach" });
    const plain = record({ name: "plain" });

    const { series, singles } = groupBySeries([pdf, reach, docx, plain]);

    expect(series).toHaveLength(1);
    expect(series[0]).toMatchObject({
      key: "anthropics/skills",
      repository: "anthropics/skills",
      url: "https://github.com/anthropics/skills/tree/main/skills",
      linked: true,
    });
    expect(series[0].skills.map((skill) => skill.name)).toEqual(["pdf", "docx"]);
    expect(singles.map((skill) => skill.name)).toEqual(["agent-reach", "plain"]);
  });

  it("groups three or more unsourced skills that share a name prefix", () => {
    const names = ["remotion-render", "remotion-docs", "Remotion-Maps", "solo-skill", "solo-other"];
    const known = record({ name: "remotion-known", sourceUrl: "https://github.com/a/b/tree/main/remotion-known" });
    const { series, singles } = groupBySeries([...names.map((name) => record({ name })), known]);

    expect(series).toHaveLength(1);
    expect(series[0]).toMatchObject({ key: "prefix:remotion", repository: "remotion-*", url: "", byPrefix: true });
    expect(series[0].skills.map((skill) => skill.name)).toEqual(["remotion-render", "remotion-docs", "Remotion-Maps"]);
    // Two of a prefix is not a series; a skill with a known source keeps it.
    expect(singles.map((skill) => skill.name)).toEqual(["solo-skill", "solo-other", "remotion-known"]);
  });

  it("points a series at the repository root when its skills share no folder", () => {
    const first = record({ name: "a", sourceUrl: "https://github.com/o/r/tree/main/a" });
    const second = record({ name: "b", sourceUrl: "https://github.com/o/r/tree/dev/b" });
    expect(groupBySeries([first, second]).series[0].url).toBe("https://github.com/o/r");
  });
});

describe("Codex-managed and local collections", () => {
  const base = {
    description: "",
    source: "Codex",
    health: "healthy" as const,
    targets: [],
    supportFiles: ["SKILL.md"],
  };
  const plugin = (name: string, version: string) => ({
    ...base,
    id: name,
    name,
    sourcePath: `C:/x/${name}`,
    origin: {
      kind: "plugin" as const,
      plugin: { id: "openai/github", name: "GitHub", version, publisher: "OpenAI", repository: null },
    },
  });

  it("groups plugin skills by plugin and system skills together", () => {
    const system = { ...base, id: "s", name: "skill-installer", sourcePath: "C:/x/s", origin: { kind: "system" as const, plugin: null } };
    const groups = groupManagedByCodex([plugin("gh-fix-ci", "0.1.10"), plugin("gh-address-comments", "0.1.10"), system], "Codex system skills");
    expect(groups.map((group) => [group.repository, group.skills.length, group.detail ?? "", group.url])).toEqual([
      ["Codex system skills", 1, "", ""],
      ["GitHub", 2, "v0.1.10 · OpenAI", ""],
    ]);
  });

  it("groups a local repository collection by its name", () => {
    const local = (name: string) => ({
      ...base,
      id: name,
      name,
      sourcePath: `C:/x/${name}`,
      origin: {
        kind: "collection" as const,
        plugin: null,
        repository: { id: "local/kit", name: "kit", url: null },
      },
    });
    const { series, singles } = groupBySeries([local("a"), local("b"), { ...base, id: "c", name: "c", sourcePath: "C:/x/c" }]);
    expect(series.map((group) => [group.key, group.repository, group.url, group.skills.length])).toEqual([
      ["collection:local/kit", "kit", "", 2],
    ]);
    expect(singles.map((record) => record.name)).toEqual(["c"]);
  });

  it("treats records without an origin as standalone and only plugin/system ones as Codex-managed", () => {
    expect(originKind({ ...base, id: "a", name: "a", sourcePath: "C:/x/a" })).toBe("standalone");
    expect(isManagedByCodex(plugin("p", "1"))).toBe(true);
    expect(isManagedByCodex({ ...base, id: "a", name: "a", sourcePath: "C:/x/a" })).toBe(false);
  });
});
