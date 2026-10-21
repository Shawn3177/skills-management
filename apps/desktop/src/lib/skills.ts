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

export type ToolId =
  | "claude-code"
  | "codex"
  | "gemini-cli"
  | "grok-build"
  | "opencode"
  | "openclaw"
  | "hermes"
  | "pi"
  | "qwen-code"
  | "qoder"
  | "codebuddy"
  | "workbuddy";

export interface Tool {
  id: ToolId;
  name: string;
  /** Scanner source label for skills found in this tool's own folder. */
  sourceLabel: string;
  mono: string;
  color: string;
  /** Its personal skills folder, for display. */
  folder: string;
}

export const LIBRARY_SOURCE = "Shared Library";

/** Tools whose skills Skills Manage can switch on and off, in display order. */
export const allTools: Tool[] = [
  { id: "claude-code", name: "Claude Code", sourceLabel: "Claude Code", mono: "C", color: "#d97757", folder: "~/.claude/skills" },
  { id: "codex", name: "Codex", sourceLabel: "Codex", mono: "X", color: "#1f2937", folder: "~/.codex/skills" },
  { id: "gemini-cli", name: "Gemini CLI", sourceLabel: "Gemini CLI", mono: "G", color: "#3186ff", folder: "~/.gemini/skills" },
  { id: "grok-build", name: "Grok Build", sourceLabel: "Grok Build", mono: "Gr", color: "#111111", folder: "~/.grok/skills" },
  { id: "opencode", name: "OpenCode", sourceLabel: "OpenCode", mono: "O", color: "#111111", folder: "~/.config/opencode/skills" },
  { id: "openclaw", name: "OpenClaw", sourceLabel: "OpenClaw", mono: "Oc", color: "#ff4d4d", folder: "~/.openclaw/skills" },
  { id: "hermes", name: "Hermes", sourceLabel: "Hermes", mono: "H", color: "#111111", folder: "~/.hermes/skills" },
  { id: "pi", name: "Pi", sourceLabel: "Pi", mono: "Pi", color: "#111111", folder: "~/.pi/agent/skills" },
  { id: "qwen-code", name: "Qwen Code", sourceLabel: "Qwen Code", mono: "Q", color: "#615ced", folder: "~/.qwen/skills" },
  { id: "qoder", name: "Qoder", sourceLabel: "Qoder", mono: "Qd", color: "#2adb5c", folder: "~/.qoder/skills" },
  { id: "codebuddy", name: "CodeBuddy", sourceLabel: "CodeBuddy", mono: "CB", color: "#6c4cff", folder: "~/.codebuddy/skills" },
  { id: "workbuddy", name: "WorkBuddy", sourceLabel: "WorkBuddy", mono: "WB", color: "#22c38e", folder: "~/.workbuddy/skills" },
];

/** Always shown unless the user hides them. */
const ALWAYS_SHOWN: ToolId[] = ["claude-code", "codex"];
const SHOWN_KEY = "skills-manage.tools.shown";
let installedTools: string[] = [];

/** The user's own show/hide choices (per computer; storage may be unavailable). */
function readShownChoices(): Record<string, boolean> {
  try {
    const saved: unknown = JSON.parse(window.localStorage.getItem(SHOWN_KEY) ?? "{}");
    return saved && typeof saved === "object" ? (saved as Record<string, boolean>) : {};
  } catch {
    return {};
  }
}

export function isToolInstalled(id: ToolId): boolean {
  return installedTools.includes(id);
}

/** Shown: the user's choice, else the main two plus every installed tool. */
export function isToolShown(id: ToolId): boolean {
  return readShownChoices()[id] ?? (ALWAYS_SHOWN.includes(id) || isToolInstalled(id));
}

/**
 * The tools the UI shows (tabs, toggles). A live binding, refreshed when the
 * installed tools are known or the user changes what is shown.
 */
export let tools: Tool[] = allTools.filter((tool) => ALWAYS_SHOWN.includes(tool.id));

function refreshShownTools() {
  tools = allTools.filter((tool) => isToolShown(tool.id));
}

export function showInstalledTools(installed: string[]) {
  installedTools = installed;
  refreshShownTools();
}

export function setToolShown(id: ToolId, shown: boolean) {
  const choices = readShownChoices();
  choices[id] = shown;
  try {
    window.localStorage.setItem(SHOWN_KEY, JSON.stringify(choices));
  } catch {
    // Not remembered; the change still applies until the app restarts.
  }
  refreshShownTools();
}

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
  /** The deepest GitHub folder that holds every skill of the series ("" when unknown). */
  url: string;
  skills: SkillRecord[];
  /** At least one member is only grouped because the user linked the repository. */
  linked: boolean;
  /** Grouped by a shared name prefix (`remotion-*`) because no source is known yet. */
  byPrefix?: boolean;
}

/** Skills without a known source that share a name prefix this often form a series. */
const PREFIX_SERIES_MIN = 3;

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

  // No source yet (not identified, offline): skills installed as a set
  // usually share a prefix, e.g. remotion-render, remotion-docs …
  const byPrefix = new Map<string, SkillRecord[]>();
  for (const record of records) {
    const dash = record.name.indexOf("-");
    if (grouped.has(record) || githubSource(record) || dash <= 0) {
      continue;
    }
    const prefix = record.name.slice(0, dash).toLowerCase();
    byPrefix.set(prefix, [...(byPrefix.get(prefix) ?? []), record]);
  }
  for (const [prefix, members] of [...byPrefix].sort(([left], [right]) => left.localeCompare(right))) {
    if (members.length < PREFIX_SERIES_MIN) {
      continue;
    }
    members.forEach((member) => grouped.add(member));
    series.push({ key: `prefix:${prefix}`, repository: `${prefix}-*`, url: "", skills: members, linked: false, byPrefix: true });
  }

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
