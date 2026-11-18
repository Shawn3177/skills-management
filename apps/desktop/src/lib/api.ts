import { invoke } from "@tauri-apps/api/core";
import type { CompatFinding, CompatLevel, SkillRecord, SkillVariant, ToolId, VariantChoice } from "./skills";

export type AppStatus = {
  appName: string;
  version: string;
  dataRoot: string;
  safeWriteMode: string;
  githubTokenConfigured: boolean;
  homeDir: string;
  /** Tool ids whose config folder exists on this computer. */
  installedTools?: string[];
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
  /** Enabled from the tool's own version: its folder in the repository. */
  variant?: string | null;
};

export type CompatReport = {
  targetId: string;
  targetName: string;
  level: CompatLevel;
  /** The frontmatter `compatibility` text, when the author wrote one. */
  declared: string | null;
  findings: CompatFinding[];
  acknowledged: boolean;
};

export type VariantChoiceResult = { targetId: string; choice: VariantChoice; refreshed: boolean; message: string };

export type DetectVariantsResult = {
  skillName: string;
  variants: SkillVariant[];
  refreshedTargets: string[];
  failedTargets: string[];
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

export type FindSourcesResult = {
  found: Array<{ folder: string; repository: string; url: string; exact: boolean }>;
  unmatched: string[];
  ambiguous: string[];
  repositories: string[];
  failed: string[];
  rateLimited: boolean;
};

export type GithubAccount = {
  connected: boolean;
  /** Missing while connected means the token no longer works. */
  login: string | null;
  name: string | null;
  source: "stored" | "environment" | null;
};

export type CatalogSkill = { folder: string; path: string; url: string; blob: string };

export type CatalogRepo = {
  fullName: string;
  description: string | null;
  htmlUrl: string;
  defaultBranch: string;
  owned: boolean;
  starred: boolean;
  fork: boolean;
  stars: number;
  skills: CatalogSkill[];
  private?: boolean;
  language?: string | null;
  pushedAt?: string | null;
};

/** The connected account's own and starred repositories that hold skills. */
export type GithubCatalog = {
  login: string;
  name?: string | null;
  avatarUrl?: string | null;
  fetchedAt: number;
  repos: CatalogRepo[];
  starred: string[];
  scanned: number;
  rateLimited: boolean;
};

export type BatchImportResult = {
  imported: Array<{ skillName: string; libraryPath: string; url: string }>;
  failed: Array<{ url: string; error: string }>;
};

export type ImportPackResult = { imported: number; skillCount: number; libraryPaths: string[] };

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

export type Project = { name: string; path: string };
export type PublicSkill = { id: string; source: string; skillId: string; name: string; installs: number };
export type PublicSearchResults = { skills: PublicSkill[]; unsupported: number };
export type PublicResolution = { repositoryId: string; paths: string[]; suggestedPath: string | null };
export type PublicPreview = {
  previewId: string;
  name: string;
  description: string;
  source: string;
  path: string;
  commit: string;
  url: string;
  markdown: string;
  files: string[];
  installedPath: string | null;
};
export type ProjectTarget = {
  id: ToolId;
  path: string;
  state: "available" | "deployed" | "outdated" | "modified" | "conflict";
  fingerprint?: string | null;
};
export type ProjectSnapshot = {
  library: Array<{ skill: SkillRecord; targets: ProjectTarget[] }>;
  local: SkillRecord[];
};

export const api = {
  searchPublicSkills: (query: string) => invoke<PublicSearchResults>("search_public_skills", { query }),
  resolvePublicSkill: (source: string, skillId: string) => invoke<PublicResolution>("resolve_public_skill", { source, skillId }),
  previewPublicSkill: (repositoryId: string, path: string) => invoke<PublicPreview>("preview_public_skill", { repositoryId, path }),
  importPublicSkill: (previewId: string) => invoke<{ libraryPath: string; alreadyInstalled: boolean }>("import_public_skill", { previewId }),
  listProjects: () => invoke<Project[]>("list_projects"),
  addProject: (path: string) => invoke<Project>("add_project", { path }),
  forgetProject: (path: string) => invoke<Project[]>("forget_project", { path }),
  scanProject: (path: string) => invoke<ProjectSnapshot>("scan_project", { path }),
  deployProjectSkill: (projectPath: string, libraryPath: string, targetId: ToolId, enabled: boolean, expectedFingerprint?: string) =>
    invoke<void>("deploy_project_skill", { projectPath, libraryPath, targetId, enabled, expectedFingerprint }),
  importProjectSkill: (projectPath: string, sourcePath: string) =>
    invoke<ImportResult>("import_project_skill", { projectPath, sourcePath }),
  scanSkills: () => invoke<SkillRecord[]>("scan_skills"),
  scanFolder: (folder: string) => invoke<SkillRecord[]>("scan_skill_folder", { folder }),
  appStatus: () => invoke<AppStatus>("get_app_status"),
  importSkill: (sourcePath: string) => invoke<ImportResult>("import_skill_to_library", { sourcePath }),
  setTarget: (sourcePath: string, targetId: ToolId, enabled: boolean, replaceExisting = false) =>
    invoke<TargetToggleResult>(
      "set_skill_target_enabled",
      replaceExisting ? { sourcePath, targetId, enabled, replaceExisting } : { sourcePath, targetId, enabled },
    ),
  checkCompat: (sourcePath: string, targetId: ToolId) => invoke<CompatReport>("check_skill_compat", { sourcePath, targetId }),
  acknowledgeCompat: (sourcePath: string, targetId: ToolId) =>
    invoke<CompatReport>("acknowledge_skill_compat", { sourcePath, targetId }),
  updateMetadata: (libraryPath: string, name: string, description: string) =>
    invoke<{ name: string; refreshedTargets: string[]; failedTargets: string[] }>("update_skill_metadata", {
      libraryPath,
      name,
      description,
    }),
  removeFromLibrary: (libraryPath: string) => invoke<RemoveResult>("remove_skill_from_library", { libraryPath }),
  importFromGithub: (url: string) => invoke<GithubImportResult>("import_from_github", { url }),
  linkSeries: (url: string) => invoke<LinkSeriesResult>("link_github_series", { url }),
  importGithubSkills: (urls: string[]) => invoke<BatchImportResult>("import_github_skills", { urls }),
  githubAccount: () => invoke<GithubAccount>("github_account"),
  connectGithubToken: (token: string) => invoke<GithubAccount>("connect_github_token", { token }),
  connectGithubCli: () => invoke<GithubAccount>("connect_github_cli"),
  disconnectGithub: () => invoke<void>("disconnect_github"),
  githubCatalog: (refresh: boolean) => invoke<GithubCatalog | null>("github_catalog", { refresh }),
  readSkillMarkdown: (skillPath: string) => invoke<string>("read_skill_markdown", { skillPath }),
  findSources: () => invoke<FindSourcesResult>("find_skill_sources"),
  unlinkSeries: (repository: string) => invoke<number>("unlink_github_series", { repository }),
  checkUpdates: () => invoke<SkillUpdateStatus[]>("check_skill_updates"),
  setVariantChoice: (libraryPath: string, targetId: ToolId, choice: VariantChoice) =>
    invoke<VariantChoiceResult>("set_skill_variant_choice", { libraryPath, targetId, choice }),
  detectVariants: (libraryPath: string) => invoke<DetectVariantsResult>("detect_skill_variants", { libraryPath }),
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
