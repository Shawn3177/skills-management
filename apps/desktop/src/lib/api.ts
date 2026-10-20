import { invoke } from "@tauri-apps/api/core";
import type { SkillRecord, ToolId } from "./skills";

export type AppStatus = {
  appName: string;
  version: string;
  dataRoot: string;
  safeWriteMode: string;
  githubTokenConfigured: boolean;
  homeDir: string;
};

export type ImportResult = {
  imported: boolean;
  alreadyManaged: boolean;
  skillName: string;
  libraryPath: string;
  message: string;
};

export type TargetToggleResult = {
  targetId: string;
  targetName: string;
  skillName: string;
  enabled: boolean;
  changed: boolean;
  targetPath: string;
  message: string;
};

export type RemoveResult = { skillName: string; removedFrom: string[]; message: string };

export type GithubImportResult = { skillName: string; libraryPath: string; url: string };

export type LinkSeriesResult = {
  repository: string;
  url: string;
  skillCount: number;
  matched: string[];
  truncated: boolean;
};

export type ImportPackResult ={ imported: number; skillCount: number; libraryPaths: string[] };

export type UpdateState =
  | "up-to-date"
  | "update-available"
  | "source-detected"
  | "source-unavailable"
  | "rate-limited"
  | "error";

export type SkillUpdateStatus = {
  libraryPath: string;
  skillName: string;
  state: UpdateState;
  hasUpdate: boolean;
  current: string;
  latest: string;
  url: string;
  message: string;
};

export type TrashSummary = { path: string; entries: number; totalBytes: number; purgeableEntries: number };

export type PurgeResult = { removed: number; failed: number };

export const api = {
  scanSkills: () => invoke<SkillRecord[]>("scan_skills"),
  scanFolder: (folder: string) => invoke<SkillRecord[]>("scan_skill_folder", { folder }),
  appStatus: () => invoke<AppStatus>("get_app_status"),
  importSkill: (sourcePath: string) => invoke<ImportResult>("import_skill_to_library", { sourcePath }),
  setTarget: (sourcePath: string, targetId: ToolId, enabled: boolean, replaceExisting = false) =>
    invoke<TargetToggleResult>(
      "set_skill_target_enabled",
      replaceExisting ? { sourcePath, targetId, enabled, replaceExisting } : { sourcePath, targetId, enabled },
    ),
  updateMetadata: (libraryPath: string, name: string, description: string) =>
    invoke<{ name: string; refreshedTargets: string[]; failedTargets: string[] }>("update_skill_metadata", {
      libraryPath,
      name,
      description,
    }),
  removeFromLibrary: (libraryPath: string) => invoke<RemoveResult>("remove_skill_from_library", { libraryPath }),
  importFromGithub: (url: string) => invoke<GithubImportResult>("import_from_github", { url }),
  linkSeries: (url: string) => invoke<LinkSeriesResult>("link_github_series", { url }),
  unlinkSeries: (repository: string) => invoke<number>("unlink_github_series", { repository }),
  checkUpdates: () => invoke<SkillUpdateStatus[]>("check_skill_updates"),
  updateSkill: (libraryPath: string) => invoke<{ skillName: string }>("update_skill_from_github", { libraryPath }),
  exportSkillpack: (sources: Array<{ sourcePath: string; name: string; targets: Array<{ id: string; enabled: boolean }> }>, destination: string) =>
    invoke<{ skillCount: number }>("export_skillpack", { sources, destination }),
  importSkillpack: (packagePath: string) => invoke<ImportPackResult>("import_skillpack", { packagePath }),
  trashSummary: (olderThanDays: number) => invoke<TrashSummary>("get_trash_summary", { olderThanDays }),
  purgeTrash: (olderThanDays: number) => invoke<PurgeResult>("purge_trash", { olderThanDays }),
};

/** True when there is no Tauri backend (the page runs in a plain browser). */
export function isBridgeUnavailable(error: unknown): boolean {
  const message = (error instanceof Error ? error.message : String(error ?? "")).toLowerCase();
  return message.includes("invoke") || message.includes("tauri") || message.includes("internals");
}

/** Tauri rejects with the backend's `Err(String)` itself. */
export function errorText(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  return error instanceof Error ? error.message : String(error ?? "");
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}
