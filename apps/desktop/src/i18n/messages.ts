export const locales = ["zh-CN", "en-US"] as const;

export type Locale = (typeof locales)[number];

export const defaultLocale: Locale = "zh-CN";

export const localeLabels: Record<Locale, string> = {
  "zh-CN": "中文",
  "en-US": "EN",
};

export type MessageKey =
  | "app.title"
  | "regions.appControls"
  | "regions.primaryNavigation"
  | "regions.skillsLibrary"
  | "regions.librarySummary"
  | "regions.discoveredSkills"
  | "regions.skillDetail"
  | "regions.skillActions"
  | "regions.importWorkspace"
  | "regions.packagesWorkspace"
  | "regions.settingsWorkspace"
  | "regions.appStatus"
  | "language.label"
  | "nav.skills"
  | "nav.import"
  | "nav.packages"
  | "nav.settings"
  | "nav.imported"
  | "search.label"
  | "search.placeholder"
  | "actions.scan"
  | "actions.importToLibrary"
  | "actions.importAllToLibrary"
  | "actions.importing"
  | "actions.alreadyInLibrary"
  | "actions.backToSkills"
  | "actions.repair"
  | "actions.exportSkillpack"
  | "actions.repairPreview"
  | "actions.exportingSkillpack"
  | "actions.exportedSkillpack"
  | "actions.importingSkillpack"
  | "actions.importedSkillpack"
  | "actions.importFromGithub"
  | "actions.importingGithub"
  | "actions.importedGithub"
  | "actions.checkUpdates"
  | "actions.checkingUpdates"
  | "actions.updatesFound"
  | "actions.noUpdates"
  | "actions.updateSelected"
  | "actions.updatingSkills"
  | "actions.updatedSkills"
  | "actions.updatedSkillsPartial"
  | "actions.exportLibraryEmpty"
  | "actions.enable"
  | "actions.disable"
  | "actions.saving"
  | "actions.enablingSkillForTarget"
  | "actions.disablingSkillForTarget"
  | "actions.enabledSkillForTarget"
  | "actions.disabledSkillForTarget"
  | "actions.importingSkill"
  | "actions.importedSkill"
  | "actions.importingAllSkills"
  | "actions.importedAllSkills"
  | "actions.importedAllSkillsPartial"
  | "actions.importAllEmpty"
  | "actions.enableAll"
  | "actions.disableAll"
  | "actions.enableAllForTarget"
  | "actions.disableAllForTarget"
  | "actions.enablingAllForTarget"
  | "actions.disablingAllForTarget"
  | "actions.enabledAllForTarget"
  | "actions.disabledAllForTarget"
  | "actions.enabledAllForTargetPartial"
  | "actions.disabledAllForTargetPartial"
  | "actions.bulkTargetNoChange"
  | "actions.skillAlreadyManaged"
  | "status.scan.scanning"
  | "status.scan.error"
  | "status.scan.browserPreview"
  | "status.scan.fallback"
  | "status.scan.ready"
  | "status.readyCompact"
  | "status.empty.title"
  | "status.empty.body"
  | "workspace.import.title"
  | "workspace.import.body"
  | "workspace.import.empty"
  | "workspace.import.emptyHint"
  | "workspace.import.bulkActions"
  | "workspace.packages.title"
  | "workspace.packages.body"
  | "workspace.packages.githubLabel"
  | "workspace.packages.githubHint"
  | "workspace.packages.updatesLabel"
  | "workspace.packages.noGithubSkills"
  | "updates.available"
  | "updates.upToDate"
  | "updates.unavailable"
  | "updates.rateLimited"
  | "updates.error"
  | "workspace.settings.title"
  | "workspace.settings.body"
  | "workspace.settings.backupValue"
  | "health.healthy"
  | "health.warning"
  | "health.broken"
  | "drawer.selectedSkill"
  | "detail.metadata"
  | "detail.source"
  | "detail.path"
  | "detail.supportFiles"
  | "detail.targets"
  | "targets.enabled"
  | "targets.disabled"
  | "targets.unavailable"
  | "targets.pending"
  | "targets.importRequiredAction"
  | "targets.pendingAction"
  | "footer.dataRoot"
  | "footer.backupMode"
  | "footer.packageFormat"
  | "errors.scanFallback"
  | "errors.importFallback"
  | "errors.targetFallback"
  | "errors.targetConflict"
  | "errors.bridgeAction"
  | "errors.exportFallback"
  | "errors.importPackFallback"
  | "errors.githubImportFailed";

export type MessageParams = Record<string, string | number>;

export const messages: Record<Locale, Record<MessageKey, string>> = {
  "zh-CN": {
    "app.title": "Skills Manage",
    "regions.appControls": "应用控制",
    "regions.primaryNavigation": "主导航",
    "regions.skillsLibrary": "技能库",
    "regions.librarySummary": "库概览",
    "regions.discoveredSkills": "已发现技能",
    "regions.skillDetail": "技能详情",
    "regions.skillActions": "技能操作",
    "regions.importWorkspace": "已导入工作区",
    "regions.packagesWorkspace": "包工作区",
    "regions.settingsWorkspace": "设置工作区",
    "regions.appStatus": "应用状态",
    "language.label": "语言",
    "nav.skills": "技能",
    "nav.import": "导入",
    "nav.packages": "包",
    "nav.settings": "设置",
    "nav.imported": "已导入",
    "search.label": "搜索技能",
    "search.placeholder": "搜索技能、来源、路径",
    "actions.scan": "扫描本地技能",
    "actions.importToLibrary": "导入共享库",
    "actions.importAllToLibrary": "全部导入",
    "actions.importing": "导入中",
    "actions.alreadyInLibrary": "已在共享库",
    "actions.backToSkills": "返回技能列表",
    "actions.repair": "修复",
    "actions.exportSkillpack": "导出 .skillpack",
    "actions.repairPreview": "{skillName} 的修复检查正在等待后端接入。",
    "actions.exportingSkillpack": "正在导出 .skillpack…",
    "actions.exportedSkillpack": "已导出 {count} 个技能到 .skillpack。",
    "actions.importingSkillpack": "正在导入 .skillpack…",
    "actions.importedSkillpack": "已从 .skillpack 导入 {count} 个技能。",
    "actions.importFromGithub": "从 GitHub 导入",
    "actions.importingGithub": "正在从 GitHub 导入…",
    "actions.importedGithub": "已从 GitHub 导入 {skillName}。",
    "actions.checkUpdates": "检查更新",
    "actions.checkingUpdates": "正在检查更新…",
    "actions.updatesFound": "发现 {count} 个可更新的技能。",
    "actions.noUpdates": "所有技能都是最新的。",
    "actions.updateSelected": "更新所选（{count}）",
    "actions.updatingSkills": "正在更新…",
    "actions.updatedSkills": "已更新 {count} 个技能。",
    "actions.updatedSkillsPartial": "已更新 {count} 个技能，{failed} 个失败。",
    "actions.exportLibraryEmpty": "共享库为空，没有可导出的技能。",
    "actions.enable": "启用",
    "actions.disable": "停用",
    "actions.saving": "保存中",
    "actions.enablingSkillForTarget": "正在为 {targetName} 启用 {skillName}。",
    "actions.disablingSkillForTarget": "正在为 {targetName} 停用 {skillName}。",
    "actions.enabledSkillForTarget": "已为 {targetName} 启用 {skillName}。",
    "actions.disabledSkillForTarget": "已为 {targetName} 停用 {skillName}。",
    "actions.importingSkill": "正在将 {skillName} 导入共享库。",
    "actions.importedSkill": "已将 {skillName} 导入共享库。",
    "actions.importingAllSkills": "正在导入 {current}/{total}：{skillName}。",
    "actions.importedAllSkills": "已导入 {count} 个技能到共享库。",
    "actions.importedAllSkillsPartial": "已导入 {count} 个技能到共享库，{failed} 个失败。",
    "actions.importAllEmpty": "没有需要导入的技能。",
    "actions.enableAll": "全部启用",
    "actions.disableAll": "全部停用",
    "actions.enableAllForTarget": "为 {targetName} 全部启用",
    "actions.disableAllForTarget": "为 {targetName} 全部停用",
    "actions.enablingAllForTarget": "正在为 {targetName} 批量启用…",
    "actions.disablingAllForTarget": "正在为 {targetName} 批量停用…",
    "actions.enabledAllForTarget": "已为 {targetName} 启用 {count} 个技能。",
    "actions.disabledAllForTarget": "已为 {targetName} 停用 {count} 个技能。",
    "actions.enabledAllForTargetPartial": "已为 {targetName} 启用 {count} 个技能，{failed} 个失败。",
    "actions.disabledAllForTargetPartial": "已为 {targetName} 停用 {count} 个技能，{failed} 个失败。",
    "actions.bulkTargetNoChange": "{targetName} 无需更改。",
    "actions.skillAlreadyManaged": "{skillName} 已在共享库中。",
    "status.scan.scanning": "正在扫描本地文件夹",
    "status.scan.error": "扫描失败，正在显示示例记录。",
    "status.scan.browserPreview": "桌面扫描在应用窗口中运行，当前显示示例记录。",
    "status.scan.fallback": "未找到本地技能，正在显示示例记录。",
    "status.scan.ready": "扫描完成，正在显示本地技能。",
    "status.readyCompact": "{total} 个技能 · {healthy} 健康 · {warnings} 待检查",
    "status.empty.title": "没有匹配的技能。",
    "status.empty.body": "试试工具名、文件夹或支持文件。",
    "workspace.import.title": "已导入技能",
    "workspace.import.body": "管理已导入共享库的技能，并为每个工具开启或关闭。",
    "workspace.import.empty": "还没有导入任何技能。",
    "workspace.import.emptyHint": "前往「技能」页导入本地技能。",
    "workspace.import.bulkActions": "批量操作",
    "workspace.packages.title": ".skillpack 包",
    "workspace.packages.body": "导入和导出 .skillpack：把共享库打包带到另一台电脑，或导入已有的技能包。",
    "workspace.packages.githubLabel": "从 GitHub 导入技能",
    "workspace.packages.githubHint": "粘贴公开仓库地址，或指向某个技能文件夹的 /tree/<分支>/<子目录> 地址。仅支持公开仓库。",
    "workspace.packages.updatesLabel": "检查 GitHub 更新",
    "workspace.packages.noGithubSkills": "还没有从 GitHub 导入的技能。",
    "updates.available": "有更新",
    "updates.upToDate": "最新",
    "updates.unavailable": "来源失效",
    "updates.rateLimited": "已限流",
    "updates.error": "出错",
    "workspace.settings.title": "本地设置",
    "workspace.settings.body": "查看数据目录、备份策略和包格式。写入类设置会在后端安全策略完成后开放。",
    "workspace.settings.backupValue": "托管写入前备份",
    "health.healthy": "健康",
    "health.warning": "需检查",
    "health.broken": "损坏",
    "drawer.selectedSkill": "已选择技能",
    "detail.metadata": "元数据",
    "detail.source": "来源",
    "detail.path": "路径",
    "detail.supportFiles": "支持文件",
    "detail.targets": "目标工具",
    "targets.enabled": "托管副本已启用",
    "targets.disabled": "未为此工具启用",
    "targets.unavailable": "先导入共享库后可启用",
    "targets.pending": "此工具开关待接入",
    "targets.importRequiredAction": "先将 {skillName} 导入共享库，再为 {targetName} 启用或停用。",
    "targets.pendingAction": "{targetName} 的启用开关正在等待后端接入。",
    "footer.dataRoot": "数据根目录",
    "footer.backupMode": "备份模式",
    "footer.packageFormat": "包格式",
    "errors.scanFallback": "无法扫描本地文件夹。",
    "errors.importFallback": "导入失败。请检查技能文件夹后重试。",
    "errors.targetFallback": "无法更新 {targetName}。请检查目标文件夹后重试。",
    "errors.targetConflict": "目标目录已存在，且不由 Skills Manage 管理。",
    "errors.bridgeAction": "该操作需要在桌面应用窗口中运行。",
    "errors.exportFallback": "导出失败，请重试。",
    "errors.importPackFallback": "导入 .skillpack 失败，文件可能无效或损坏。",
    "errors.githubImportFailed": "从 GitHub 导入失败：{reason}",
  },
  "en-US": {
    "app.title": "Skills Manage",
    "regions.appControls": "Application controls",
    "regions.primaryNavigation": "Primary navigation",
    "regions.skillsLibrary": "Skills library",
    "regions.librarySummary": "Library summary",
    "regions.discoveredSkills": "Discovered skills",
    "regions.skillDetail": "Skill detail",
    "regions.skillActions": "Skill actions",
    "regions.importWorkspace": "Imported workspace",
    "regions.packagesWorkspace": "Packages workspace",
    "regions.settingsWorkspace": "Settings workspace",
    "regions.appStatus": "App status",
    "language.label": "Language",
    "nav.skills": "Skills",
    "nav.import": "Import",
    "nav.packages": "Packages",
    "nav.settings": "Settings",
    "nav.imported": "Imported",
    "search.label": "Search skills",
    "search.placeholder": "Search skills, sources, paths",
    "actions.scan": "Scan local skills",
    "actions.importToLibrary": "Import to library",
    "actions.importAllToLibrary": "Import all",
    "actions.importing": "Importing",
    "actions.alreadyInLibrary": "Already in library",
    "actions.backToSkills": "Back to skills",
    "actions.repair": "Repair",
    "actions.exportSkillpack": "Export .skillpack",
    "actions.repairPreview": "Repair checks for {skillName} are waiting for the repair backend.",
    "actions.exportingSkillpack": "Exporting .skillpack…",
    "actions.exportedSkillpack": "Exported {count} skills to .skillpack.",
    "actions.importingSkillpack": "Importing .skillpack…",
    "actions.importedSkillpack": "Imported {count} skills from .skillpack.",
    "actions.importFromGithub": "Import from GitHub",
    "actions.importingGithub": "Importing from GitHub…",
    "actions.importedGithub": "Imported {skillName} from GitHub.",
    "actions.checkUpdates": "Check for updates",
    "actions.checkingUpdates": "Checking for updates…",
    "actions.updatesFound": "{count} skill(s) have updates.",
    "actions.noUpdates": "All skills are up to date.",
    "actions.updateSelected": "Update selected ({count})",
    "actions.updatingSkills": "Updating…",
    "actions.updatedSkills": "Updated {count} skill(s).",
    "actions.updatedSkillsPartial": "Updated {count} skill(s), {failed} failed.",
    "actions.exportLibraryEmpty": "The shared library has no skills to export.",
    "actions.enable": "Enable",
    "actions.disable": "Disable",
    "actions.saving": "Saving",
    "actions.enablingSkillForTarget": "Enabling {skillName} for {targetName}.",
    "actions.disablingSkillForTarget": "Disabling {skillName} for {targetName}.",
    "actions.enabledSkillForTarget": "Enabled {skillName} for {targetName}.",
    "actions.disabledSkillForTarget": "Disabled {skillName} for {targetName}.",
    "actions.importingSkill": "Importing {skillName} into the shared library.",
    "actions.importedSkill": "Imported {skillName} into the shared library.",
    "actions.importingAllSkills": "Importing {current}/{total}: {skillName}.",
    "actions.importedAllSkills": "Imported {count} skills into the shared library.",
    "actions.importedAllSkillsPartial": "Imported {count} skills into the shared library, {failed} failed.",
    "actions.importAllEmpty": "No skills need importing.",
    "actions.enableAll": "Enable all",
    "actions.disableAll": "Disable all",
    "actions.enableAllForTarget": "Enable all for {targetName}",
    "actions.disableAllForTarget": "Disable all for {targetName}",
    "actions.enablingAllForTarget": "Enabling all for {targetName}…",
    "actions.disablingAllForTarget": "Disabling all for {targetName}…",
    "actions.enabledAllForTarget": "Enabled {count} skills for {targetName}.",
    "actions.disabledAllForTarget": "Disabled {count} skills for {targetName}.",
    "actions.enabledAllForTargetPartial": "Enabled {count} skills for {targetName}, {failed} failed.",
    "actions.disabledAllForTargetPartial": "Disabled {count} skills for {targetName}, {failed} failed.",
    "actions.bulkTargetNoChange": "Nothing to change for {targetName}.",
    "actions.skillAlreadyManaged": "{skillName} is already in the shared library.",
    "status.scan.scanning": "Scanning local folders",
    "status.scan.error": "Scan failed. Showing sample records.",
    "status.scan.browserPreview": "Desktop scan runs in the app window. Showing sample records.",
    "status.scan.fallback": "No local skills found. Showing sample records.",
    "status.scan.ready": "Scan complete. Showing local skills.",
    "status.readyCompact": "{total} skills · {healthy} healthy · {warnings} review",
    "status.empty.title": "No skills match this search.",
    "status.empty.body": "Try a tool name, folder, or support file.",
    "workspace.import.title": "Imported skills",
    "workspace.import.body": "Manage skills imported into the shared library and switch them on or off per tool.",
    "workspace.import.empty": "No skills imported yet.",
    "workspace.import.emptyHint": "Go to the Skills tab to import local skills.",
    "workspace.import.bulkActions": "Bulk actions",
    "workspace.packages.title": ".skillpack packages",
    "workspace.packages.body": "Import and export .skillpack bundles: pack your shared library to move it to another machine, or import an existing bundle.",
    "workspace.packages.githubLabel": "Import a skill from GitHub",
    "workspace.packages.githubHint": "Paste a public repo URL, or a /tree/<branch>/<subdir> URL pointing at a skill folder. Public repositories only.",
    "workspace.packages.updatesLabel": "Check for GitHub updates",
    "workspace.packages.noGithubSkills": "No skills imported from GitHub yet.",
    "updates.available": "Update available",
    "updates.upToDate": "Up to date",
    "updates.unavailable": "Source unavailable",
    "updates.rateLimited": "Rate limited",
    "updates.error": "Error",
    "workspace.settings.title": "Local settings",
    "workspace.settings.body": "Review the data root, backup policy, and package format. Writeable settings open after the backend safety path is ready.",
    "workspace.settings.backupValue": "Back up before managed writes",
    "health.healthy": "Healthy",
    "health.warning": "Needs review",
    "health.broken": "Broken",
    "drawer.selectedSkill": "Selected skill",
    "detail.metadata": "Metadata",
    "detail.source": "Source",
    "detail.path": "Path",
    "detail.supportFiles": "Support files",
    "detail.targets": "Target tools",
    "targets.enabled": "Managed copy is active",
    "targets.disabled": "Not enabled for this tool",
    "targets.unavailable": "Import to the shared library before enabling",
    "targets.pending": "Target switch pending",
    "targets.importRequiredAction": "Import {skillName} into the shared library before changing {targetName}.",
    "targets.pendingAction": "{targetName} switching is waiting for the backend.",
    "footer.dataRoot": "Data root",
    "footer.backupMode": "Backup mode",
    "footer.packageFormat": "Package format",
    "errors.scanFallback": "Unable to scan local folders.",
    "errors.importFallback": "Import failed. Check the skill folder and try again.",
    "errors.targetFallback": "Could not update {targetName}. Check the target folder and try again.",
    "errors.targetConflict": "The target folder already exists and is not managed by Skills Manage.",
    "errors.bridgeAction": "This action runs in the desktop app window.",
    "errors.exportFallback": "Export failed. Please try again.",
    "errors.importPackFallback": "Could not import the .skillpack. The file may be invalid.",
    "errors.githubImportFailed": "GitHub import failed: {reason}",
  },
};

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && locales.includes(value as Locale);
}

export function getMessage(locale: Locale, key: string): string {
  const typedKey = key as MessageKey;
  return messages[locale][typedKey] ?? messages["en-US"][typedKey] ?? key;
}

export function formatMessage(locale: Locale, key: MessageKey, params: MessageParams = {}): string {
  return getMessage(locale, key).replace(/\{(\w+)\}/g, (match, paramKey: string) => {
    const value = params[paramKey];
    return value === undefined ? match : String(value);
  });
}
