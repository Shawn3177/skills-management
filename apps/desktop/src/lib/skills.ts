export type SkillHealth = "healthy" | "warning" | "broken";
export type SkillOriginKind = "standalone" | "collection" | "plugin" | "system";

export interface PluginIdentity {
  id: string;
  name: string;
  version: string;
  publisher: string;
  repository?: string | null;
}

export interface RepositoryIdentity {
  id: string;
  name: string;
  url: string | null;
}

export interface SkillOrigin {
  kind: SkillOriginKind;
  plugin: PluginIdentity | null;
  repository?: RepositoryIdentity | null;
}

export interface TargetState {
  id: string;
  name: string;
  enabled: boolean;
}

export interface SkillRecord {
  id: string;
  name: string;
  description: string;
  source: string;
  sourcePath: string;
  origin: SkillOrigin;
  health: SkillHealth;
  targets: TargetState[];
  supportFiles: string[];
}

const healthRank: Record<SkillHealth, number> = {
  healthy: 0,
  warning: 1,
  broken: 2,
};

export interface SkillStats {
  total: number;
  healthy: number;
  warnings: number;
  enabledTargets: number;
}

export function filterSkills(records: SkillRecord[], query: string): SkillRecord[] {
  const normalized = query.trim().toLowerCase();

  if (!normalized) {
    return records;
  }

  return records.filter((record) => {
    const haystack = [
      record.name,
      record.description,
      record.source,
      record.sourcePath,
      record.origin.kind,
      record.origin.plugin?.id ?? "",
      record.origin.plugin?.name ?? "",
      record.origin.plugin?.version ?? "",
      record.origin.plugin?.publisher ?? "",
      record.origin.plugin?.repository ?? "",
      record.origin.repository?.id ?? "",
      record.origin.repository?.name ?? "",
      record.origin.repository?.url ?? "",
      record.supportFiles.join(" "),
    ]
      .join(" ")
      .toLowerCase();

    return haystack.includes(normalized);
  });
}

export function getSkillStats(records: SkillRecord[]): SkillStats {
  return {
    total: records.length,
    healthy: records.filter((record) => record.health === "healthy").length,
    warnings: records.filter((record) => record.health === "warning").length,
    enabledTargets: records.reduce(
      (count, record) => count + record.targets.filter((target) => target.enabled).length,
      0,
    ),
  };
}

export interface SkillListGroup {
  id: string;
  kind: Exclude<SkillOriginKind, "standalone"> | null;
  repository: RepositoryIdentity | null;
  plugin: PluginIdentity | null;
  skills: SkillRecord[];
}

export function groupSkillsForList(records: SkillRecord[]): SkillListGroup[] {
  const groups = new Map<string, SkillListGroup>();

  for (const record of records) {
    const repository = record.origin.repository ?? null;
    const plugin = repository ? null : record.origin.plugin;
    const kind = repository
      ? "collection"
      : plugin
        ? record.origin.kind === "system"
          ? "system"
          : "plugin"
        : null;
    const key = repository
      ? "repository:" + repository.id
      : plugin
        ? kind + ":" + plugin.id
        : "skill:" + record.id;
    const existing = groups.get(key);

    if (existing) {
      existing.skills.push(record);
    } else {
      groups.set(key, {
        id: key,
        kind,
        repository,
        plugin,
        skills: [record],
      });
    }
  }

  return Array.from(groups.values())
    .map((group) => ({
      ...group,
      skills: [...group.skills].sort(compareSkillRecords),
    }))
    .sort((left, right) => compareLabels(groupDisplayName(left), groupDisplayName(right)));
}

export interface SkillListSection {
  kind: SkillOriginKind;
  skillCount: number;
  sourceCount: number;
  groups: SkillListGroup[];
}

const skillSectionOrder: SkillOriginKind[] = ["collection", "plugin", "standalone", "system"];

export function groupSkillsIntoSections(records: SkillRecord[]): SkillListSection[] {
  const groupsByKind = new Map<SkillOriginKind, SkillListGroup[]>();

  for (const group of groupSkillsForList(records)) {
    const kind = group.kind ?? group.skills[0]?.origin.kind;
    if (!kind) {
      continue;
    }

    const sectionGroups = groupsByKind.get(kind);
    if (sectionGroups) {
      sectionGroups.push(group);
    } else {
      groupsByKind.set(kind, [group]);
    }
  }

  return skillSectionOrder.flatMap((kind) => {
    const groups = groupsByKind.get(kind) ?? [];
    if (groups.length === 0) {
      return [];
    }

    return [
      {
        kind,
        skillCount: groups.reduce((count, group) => count + group.skills.length, 0),
        sourceCount: groups.length,
        groups,
      },
    ];
  });
}

export function isImportableSkill(record: SkillRecord): boolean {
  return record.origin.kind === "standalone" || record.origin.kind === "collection";
}

export function mergeSameNamedSkills(records: SkillRecord[]): SkillRecord[] {
  const groups = new Map<string, SkillRecord[]>();

  for (const record of records) {
    const key = mergeGroupKey(record);
    const group = groups.get(key);

    if (group) {
      group.push(record);
    } else {
      groups.set(key, [record]);
    }
  }

  return Array.from(groups.values()).map((group) => {
    if (group.length === 1) {
      return group[0];
    }

    const primary = group.find((record) => record.source === "Shared Library") ?? group[0];
    const sources = uniqueInOrder(group.map((record) => record.source));
    const targets = mergeTargets(group);
    const supportFiles = uniqueInOrder(group.flatMap((record) => record.supportFiles));
    const health = group.reduce<SkillHealth>(
      (current, record) => (healthRank[record.health] > healthRank[current] ? record.health : current),
      "healthy",
    );

    return {
      ...primary,
      id: mergedSkillId(group[0]),
      description: group.find((record) => record.description.trim())?.description ?? primary.description,
      health,
      source: sources.join(", "),
      targets,
      supportFiles,
    };
  });
}

export function getSkillUsageLabels(record: SkillRecord): string[] {
  const enabledTargets = uniqueInOrder(record.targets.filter((target) => target.enabled).map((target) => target.name));

  if (enabledTargets.length > 0) {
    return enabledTargets;
  }

  return record.source.split(",").map((source) => source.trim()).filter(Boolean);
}

function mergeTargets(records: SkillRecord[]): TargetState[] {
  const targets = new Map<string, TargetState>();

  for (const record of records) {
    for (const target of record.targets) {
      const existing = targets.get(target.id);

      targets.set(target.id, {
        ...target,
        name: existing?.name ?? target.name,
        enabled: Boolean(existing?.enabled || target.enabled),
      });
    }
  }

  return Array.from(targets.values());
}

function mergeGroupKey(record: SkillRecord): string {
  const originGroup =
    record.origin.kind === "standalone"
      ? "standalone"
      : record.origin.kind === "collection"
        ? `collection:${record.origin.repository?.id ?? "local"}`
        : record.origin.kind + ":" + (record.origin.plugin?.id ?? "codex-system");

  return [originGroup, normalizedMergeText(record.name), normalizedMergeText(record.description)].join("\u0000");
}

function groupDisplayName(group: SkillListGroup): string {
  return group.repository?.name ?? group.plugin?.name ?? group.skills[0]?.name ?? group.id;
}

function compareSkillRecords(left: SkillRecord, right: SkillRecord): number {
  return compareLabels(left.name, right.name) || compareLabels(left.id, right.id);
}

function compareLabels(left: string, right: string): number {
  return left.localeCompare(right, "en", { sensitivity: "base", numeric: true });
}

function normalizedMergeText(value: string): string {
  return value.trim().replace(/\s+/g, " ").toLowerCase();
}

function mergedSkillId(record: SkillRecord): string {
  return `skill:${safeIdPart(record.name)}-${stableHash(mergeGroupKey(record))}`;
}

function safeIdPart(value: string): string {
  const part = normalizedMergeText(value).replace(/[^a-z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");

  return part || "managed-skill";
}

function stableHash(value: string): string {
  let hash = 0;

  for (const character of value) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }

  return hash.toString(36);
}

function uniqueInOrder(values: string[]): string[] {
  return values.filter((value, index, array) => value.trim() && array.indexOf(value) === index);
}
