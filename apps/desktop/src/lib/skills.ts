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
