export type SkillHealth = "healthy" | "warning" | "broken";

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
  return `${normalizedMergeText(record.name)}\u0000${normalizedMergeText(record.description)}`;
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
