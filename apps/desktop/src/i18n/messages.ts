export const locales = ["zh-CN", "en-US"] as const;

export type Locale = (typeof locales)[number];

export const defaultLocale: Locale = "zh-CN";

export const localeLabels: Record<Locale, string> = {
  "zh-CN": "中文",
  "en-US": "EN",
};

type MessageKey =
  | "app.title"
  | "app.subtitle"
  | "app.safeMode"
  | "app.workflowSettings"
  | "nav.skills"
  | "nav.import"
  | "nav.packages"
  | "nav.settings"
  | "stats.skills"
  | "stats.healthy"
  | "stats.review"
  | "stats.enabledLinks"
  | "search.label"
  | "search.placeholder"
  | "actions.scan"
  | "actions.importToLibrary"
  | "actions.importing"
  | "actions.alreadyInLibrary"
  | "actions.repair"
  | "actions.exportSkillpack"
  | "actions.enable"
  | "actions.disable"
  | "actions.saving"
  | "actions.enablingSkillForTarget"
  | "actions.disablingSkillForTarget"
  | "actions.enabledSkillForTarget"
  | "actions.disabledSkillForTarget"
  | "actions.importingSkill"
  | "actions.importedSkill"
  | "actions.skillAlreadyManaged"
  | "status.scan.scanning"
  | "status.scan.error"
  | "status.scan.fallback"
  | "status.scan.ready"
  | "status.empty.title"
  | "status.empty.body"
  | "health.healthy"
  | "health.warning"
  | "health.broken"
  | "detail.eyebrow"
  | "detail.metadata"
  | "detail.source"
  | "detail.path"
  | "detail.supportFiles"
  | "detail.targets"
  | "targets.enabled"
  | "targets.disabled"
  | "targets.unavailable"
  | "footer.dataRoot"
  | "footer.backupMode"
  | "footer.packageFormat"
  | "errors.scanFallback"
  | "errors.importFallback"
  | "errors.targetFallback";

export type MessageParams = Record<string, string | number>;

export const messages: Record<Locale, Record<MessageKey, string>> = {
  "zh-CN": {
    "app.title": "Skills Manage",
    "app.subtitle": "本地 Skills 控制台",
    "app.safeMode": "预览安全模式",
    "app.workflowSettings": "工作流设置",
    "nav.skills": "技能",
    "nav.import": "导入",
    "nav.packages": "包",
    "nav.settings": "设置",
    "stats.skills": "技能",
    "stats.healthy": "健康",
    "stats.review": "需检查",
    "stats.enabledLinks": "已启用链接",
    "search.label": "搜索技能",
    "search.placeholder": "搜索技能、来源、路径",
    "actions.scan": "扫描本地技能",
    "actions.importToLibrary": "导入共享库",
    "actions.importing": "导入中",
    "actions.alreadyInLibrary": "已在共享库",
    "actions.repair": "修复",
    "actions.exportSkillpack": "导出 .skillpack",
    "actions.enable": "启用",
    "actions.disable": "停用",
    "actions.saving": "保存中",
    "actions.enablingSkillForTarget": "正在为 {targetName} 启用 {skillName}。",
    "actions.disablingSkillForTarget": "正在为 {targetName} 停用 {skillName}。",
    "actions.enabledSkillForTarget": "已为 {targetName} 启用 {skillName}。",
    "actions.disabledSkillForTarget": "已为 {targetName} 停用 {skillName}。",
    "actions.importingSkill": "正在将 {skillName} 导入共享库。",
    "actions.importedSkill": "已将 {skillName} 导入共享库。",
    "actions.skillAlreadyManaged": "{skillName} 已在共享库中。",
    "status.scan.scanning": "正在扫描本地文件夹",
    "status.scan.error": "扫描不可用：{error}",
    "status.scan.fallback": "未找到本地技能，正在显示示例记录。",
    "status.scan.ready": "扫描完成，正在显示本地技能。",
    "status.empty.title": "没有匹配的技能。",
    "status.empty.body": "试试工具名、文件夹或支持文件。",
    "health.healthy": "健康",
    "health.warning": "需检查",
    "health.broken": "损坏",
    "detail.eyebrow": "已选择技能",
    "detail.metadata": "元数据",
    "detail.source": "来源",
    "detail.path": "路径",
    "detail.supportFiles": "支持文件",
    "detail.targets": "目标工具",
    "targets.enabled": "托管副本已启用",
    "targets.disabled": "未为此工具启用",
    "targets.unavailable": "先导入共享库后可启用",
    "footer.dataRoot": "数据根目录：%USERPROFILE%\\.skills-manage",
    "footer.backupMode": "备份模式：每次托管写入前备份",
    "footer.packageFormat": "包格式：.skillpack",
    "errors.scanFallback": "无法扫描本地文件夹。",
    "errors.importFallback": "导入失败。请检查技能文件夹后重试。",
    "errors.targetFallback": "无法更新 {targetName}。请检查目标文件夹后重试。",
  },
  "en-US": {
    "app.title": "Skills Manage",
    "app.subtitle": "Local skills control surface",
    "app.safeMode": "Preview safe mode",
    "app.workflowSettings": "Workflow settings",
    "nav.skills": "Skills",
    "nav.import": "Import",
    "nav.packages": "Packages",
    "nav.settings": "Settings",
    "stats.skills": "Skills",
    "stats.healthy": "Healthy",
    "stats.review": "Review",
    "stats.enabledLinks": "Enabled links",
    "search.label": "Search skills",
    "search.placeholder": "Search skills, sources, paths",
    "actions.scan": "Scan local skills",
    "actions.importToLibrary": "Import to library",
    "actions.importing": "Importing",
    "actions.alreadyInLibrary": "Already in library",
    "actions.repair": "Repair",
    "actions.exportSkillpack": "Export .skillpack",
    "actions.enable": "Enable",
    "actions.disable": "Disable",
    "actions.saving": "Saving",
    "actions.enablingSkillForTarget": "Enabling {skillName} for {targetName}.",
    "actions.disablingSkillForTarget": "Disabling {skillName} for {targetName}.",
    "actions.enabledSkillForTarget": "Enabled {skillName} for {targetName}.",
    "actions.disabledSkillForTarget": "Disabled {skillName} for {targetName}.",
    "actions.importingSkill": "Importing {skillName} into the shared library.",
    "actions.importedSkill": "Imported {skillName} into the shared library.",
    "actions.skillAlreadyManaged": "{skillName} is already in the shared library.",
    "status.scan.scanning": "Scanning local folders",
    "status.scan.error": "Scan unavailable: {error}",
    "status.scan.fallback": "No local skills found. Showing sample records.",
    "status.scan.ready": "Scan complete. Showing local skills.",
    "status.empty.title": "No skills match this search.",
    "status.empty.body": "Try a tool name, folder, or support file.",
    "health.healthy": "Healthy",
    "health.warning": "Needs review",
    "health.broken": "Broken",
    "detail.eyebrow": "Selected skill",
    "detail.metadata": "Metadata",
    "detail.source": "Source",
    "detail.path": "Path",
    "detail.supportFiles": "Support files",
    "detail.targets": "Target tools",
    "targets.enabled": "Managed copy is active",
    "targets.disabled": "Not enabled for this tool",
    "targets.unavailable": "Import to the shared library before enabling",
    "footer.dataRoot": "Data root: %USERPROFILE%\\.skills-manage",
    "footer.backupMode": "Backup mode: before every managed write",
    "footer.packageFormat": "Package format: .skillpack",
    "errors.scanFallback": "Unable to scan local folders.",
    "errors.importFallback": "Import failed. Check the skill folder and try again.",
    "errors.targetFallback": "Could not update {targetName}. Check the target folder and try again.",
  },
};

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && locales.includes(value as Locale);
}

export function getMessage(locale: Locale, key: string): string {
  const typedKey = key as MessageKey;
  return messages[locale][typedKey] ?? messages["en-US"][typedKey] ?? key;
}

export function formatMessage(locale: Locale, key: string, params: MessageParams = {}): string {
  return getMessage(locale, key).replace(/\{(\w+)\}/g, (match, paramKey: string) => {
    const value = params[paramKey];
    return value === undefined ? match : String(value);
  });
}
