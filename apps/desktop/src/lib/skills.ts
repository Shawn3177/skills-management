export type SkillHealth = "healthy" | "warning" | "broken";

export interface TargetState {
  id: string;
  name: string;
  enabled: boolean;
  /** The tool has a same-named folder with different content (not managed). */
  conflict?: boolean;
}

export interface SkillRecord {
  id: string;
  name: string;
  description: string;
  source: string;
  sourcePath: string;
  health: SkillHealth;
  targets: TargetState[];
  supportFiles: string[];
  /** A copy Skills Manage placed in a tool folder; it mirrors a library skill. */
  managed?: boolean;
  /** Where a library skill came from, when known (its GitHub URL). */
  sourceUrl?: string | null;
  /** How a library skill got there: "github", "local" or "skillpack". */
  originKind?: string | null;
  /** The original folder (local) or package file name (skillpack). */
  originDetail?: string | null;
  /** A GitHub repository named in SKILL.md (library skills not added from GitHub). */
  detectedSourceUrl?: string | null;
  /** `detectedSourceUrl` comes from a GitHub repository the user linked. */
  linkedSource?: boolean;
  /** Specific SKILL.md problems: "unreadable", "missing-name", "missing-description". */
  issues?: string[];
}

export type ToolId = "claude-code" | "codex";

export interface Tool {
  id: ToolId;
  name: string;
  /** Scanner source label for skills found in this tool's own folder. */
  sourceLabel: string;
  mono: string;
  color: string;
}

export const LIBRARY_SOURCE = "Shared Library";

/** Tools whose skills Skills Manage can switch on and off, in display order. */
export const tools: Tool[] = [
  { id: "claude-code", name: "Claude Code", sourceLabel: "Claude Code", mono: "C", color: "#d97757" },
  { id: "codex", name: "Codex", sourceLabel: "Codex", mono: "X", color: "#1f2937" },
];

/** Shown in the tool switcher but not yet switchable. */
export const upcomingTools = [{ id: "vs-code", name: "VS Code", mono: "V" }];

export function isLibrarySkill(record: SkillRecord): boolean {
  return record.source === LIBRARY_SOURCE;
}

export function isEnabledIn(record: SkillRecord, toolId: ToolId): boolean {
  return record.targets.some((target) => target.id === toolId && target.enabled);
}

/** The GitHub repository a library skill can be updated from, if any. */
export function githubSource(record: SkillRecord): string | null {
  return record.sourceUrl ?? record.detectedSourceUrl ?? null;
}

export function hasConflictIn(record: SkillRecord, toolId: ToolId): boolean {
  return record.targets.some((target) => target.id === toolId && Boolean(target.conflict));
}

export function folderName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/**
 * Library skills, plus the skills that live only in one tool's folder (not
 * managed copies, and not a same-named folder of a library skill, which is
 * almost always the original a library skill was imported from).
 */
export function partitionSkills(records: SkillRecord[]): { library: SkillRecord[]; locals: SkillRecord[] } {
  const library = records.filter(isLibrarySkill);
  const libraryFolders = new Set(library.map((record) => folderName(record.sourcePath).toLowerCase()));
  const locals = records.filter(
    (record) =>
      !isLibrarySkill(record) &&
      !record.managed &&
      !libraryFolders.has(folderName(record.sourcePath).toLowerCase()),
  );

  return { library: sortByName(library), locals: sortByName(locals) };
}

export function enabledCount(library: SkillRecord[], toolId: ToolId): number {
  return library.filter((record) => isEnabledIn(record, toolId)).length;
}

export function filterSkills(records: SkillRecord[], query: string): SkillRecord[] {
  const normalized = query.trim().toLowerCase();

  if (!normalized) {
    return records;
  }

  return records.filter((record) =>
    [record.name, record.description, record.source, record.sourcePath, record.sourceUrl ?? ""]
      .join(" ")
      .toLowerCase()
      .includes(normalized),
  );
}

/** Show a path under the home folder as `~/…`, with forward slashes. */
export function tildePath(path: string, home: string | null | undefined): string {
  // Older origin records kept Windows' verbatim prefix (\\?\C:\...).
  const normalized = path.replace(/\\/g, "/").replace(/^\/\/\?\//, "");
  if (!home) {
    return normalized;
  }
  const normalizedHome = home.replace(/\\/g, "/").replace(/\/+$/, "");
  return normalized.toLowerCase().startsWith(`${normalizedHome.toLowerCase()}/`)
    ? `~${normalized.slice(normalizedHome.length)}`
    : normalized;
}

export function parentFolder(path: string): string {
  const parts = path.split(/[\\/]/);
  parts.pop();
  return parts.join("/") || path;
}

export type SourceLine =
  | { kind: "github"; text: string; url: string }
  | { kind: "local"; path: string }
  | { kind: "skillpack"; name: string }
  | { kind: "library"; folder: string };

/** The blue "where it came from" line under a library skill's name. */
export function sourceLine(record: SkillRecord, home?: string | null): SourceLine {
  const github = githubSource(record);
  if (github) {
    return { kind: "github", text: github.replace(/^https?:\/\//, ""), url: github };
  }
  if (record.originKind === "local" && record.originDetail) {
    return { kind: "local", path: tildePath(parentFolder(record.originDetail), home) };
  }
  if (record.originKind === "skillpack" && record.originDetail) {
    return { kind: "skillpack", name: record.originDetail };
  }
  return { kind: "library", folder: folderName(record.sourcePath) };
}

type GithubLocation = { owner: string; repo: string; ref: string | null; path: string[] };

/** Owner, repository, ref and folder of a github.com URL (null for other URLs). */
export function parseGithubLocation(url: string): GithubLocation | null {
  const match = /^(?:https?:\/\/)?(?:www\.)?github\.com\/([^/?#]+)\/([^/?#]+)(?:\/(?:tree|blob)\/([^?#]+))?/i.exec(url.trim());
  if (!match) {
    return null;
  }
  const [ref = null, ...path] = match[3] ? match[3].split("/").filter(Boolean) : [];
  return { owner: match[1], repo: match[2].replace(/\.git$/i, ""), ref, path };
}

/** Skills that come from the same GitHub repository. */
export interface SkillSeries {
  /** `owner/repo`, lower-cased. */
  key: string;
  /** `owner/repo` as written. */
  repository: string;
  /** The deepest GitHub folder that holds every skill of the series. */
  url: string;
  skills: SkillRecord[];
  /** At least one member is only grouped because the user linked the repository. */
  linked: boolean;
}

/**
 * Collect skills from the same GitHub repository into series (two or more
 * skills); everything else stays a single skill. Order is kept within each.
 */
export function groupBySeries(records: SkillRecord[]): { series: SkillSeries[]; singles: SkillRecord[] } {
  const byRepo = new Map<string, { location: GithubLocation; members: Array<{ record: SkillRecord; location: GithubLocation }> }>();
  for (const record of records) {
    const url = githubSource(record);
    const location = url ? parseGithubLocation(url) : null;
    if (!location) {
      continue;
    }
    const key = `${location.owner}/${location.repo}`.toLowerCase();
    const entry = byRepo.get(key) ?? { location, members: [] };
    entry.members.push({ record, location });
    byRepo.set(key, entry);
  }

  const series: SkillSeries[] = [];
  const grouped = new Set<SkillRecord>();
  for (const [key, { location, members }] of byRepo) {
    if (members.length < 2) {
      continue;
    }
    members.forEach((member) => grouped.add(member.record));
    series.push({
      key,
      repository: `${location.owner}/${location.repo}`,
      url: seriesUrl(location, members.map((member) => member.location)),
      skills: members.map((member) => member.record),
      linked: members.some((member) => Boolean(member.record.linkedSource)),
    });
  }
  series.sort((left, right) => left.repository.localeCompare(right.repository));
  return { series, singles: records.filter((record) => !grouped.has(record)) };
}

function seriesUrl(repo: GithubLocation, members: GithubLocation[]): string {
  const root = `https://github.com/${repo.owner}/${repo.repo}`;
  const ref = members[0].ref;
  if (!ref || members.some((member) => member.ref !== ref)) {
    return root;
  }
  // Each member points at its own folder; the series lives in their common parent.
  const parents = members.map((member) => member.path.slice(0, -1));
  const common: string[] = [];
  for (let index = 0; parents.every((parent) => index < parent.length && parent[index] === parents[0][index]); index += 1) {
    common.push(parents[0][index]);
  }
  return common.length > 0 ? `${root}/tree/${ref}/${common.join("/")}` : root;
}

/** Up to two characters for the skill's icon tile. */
export function monogram(name: string): string {
  const letters = name.replace(/[^\p{L}\p{N}]+/gu, " ").trim().split(/\s+/).filter(Boolean);
  if (letters.length === 0) {
    return "?";
  }
  const first = letters[0];
  if (/[^\x00-\x7f]/.test(first)) {
    return first.slice(0, 1);
  }
  if (letters.length === 1) {
    return first.slice(0, 2).toUpperCase();
  }
  return (letters[0][0] + letters[1][0]).toUpperCase();
}

const tilePalette = [
  ["#eef2ff", "#4338ca"],
  ["#fff7ed", "#c2410c"],
  ["#ecfeff", "#0e7490"],
  ["#f0fdf4", "#15803d"],
  ["#fdf4ff", "#a21caf"],
  ["#fefce8", "#a16207"],
] as const;

/** A stable soft color pair for a skill's tile, derived from its name. */
export function tileColors(name: string): { background: string; color: string } {
  let hash = 0;
  for (const character of name) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }
  const [background, color] = tilePalette[hash % tilePalette.length];
  return { background, color };
}

function sortByName(records: SkillRecord[]): SkillRecord[] {
  return [...records].sort((left, right) => left.name.localeCompare(right.name));
}
