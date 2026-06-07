# Bilingual CC Switch UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Chinese/English UI switching and refresh the Skills Manage desktop surface into a compact CC Switch-inspired local control panel.

**Architecture:** Keep Rust/Tauri commands unchanged and implement this as a frontend-only phase. Add a typed i18n layer with `zh-CN` as default, localStorage persistence, a segmented language switch, and localized UI chrome. Refresh `App.tsx` and `App.css` in place first, extracting only the language switch and i18n helpers so this patch stays narrow.

**Tech Stack:** React 19, TypeScript, Vite, Vitest, Testing Library, Tailwind CSS import, lucide-react icons, Tauri invoke bridge.

---

## Scope And File Map

Create:

- `apps/desktop/src/i18n/messages.ts`
  Owns locale types, locale validation, message dictionaries, and interpolation helpers.
- `apps/desktop/src/i18n/messages.test.ts`
  Verifies default locale, fallback behavior, interpolation, and message coverage.
- `apps/desktop/src/i18n/useLocale.ts`
  Owns localStorage persistence and the React hook.
- `apps/desktop/src/i18n/useLocale.test.tsx`
  Verifies persisted locale restoration, invalid locale fallback, and runtime locale updates.
- `apps/desktop/src/components/LanguageSwitch.tsx`
  Owns the segmented `中文 / EN` control.
- `apps/desktop/src/components/LanguageSwitch.test.tsx`
  Verifies accessible selected state and click behavior.

Modify:

- `apps/desktop/src/App.tsx`
  Replace hard-coded interface text with `t(...)`, add the top control bar, add `LanguageSwitch`, and keep existing scan/import/target Tauri calls.
- `apps/desktop/src/App.css`
  Convert the current dark-rail visual shell into a light, compact CC Switch-inspired desktop control surface.
- `apps/desktop/src/App.test.tsx`
  Update assertions for Chinese default UI, English switching, localized import/target messages, and unchanged skill data.

Do not modify:

- `apps/desktop/src-tauri/**`
  The current feature is renderer-only.
- `agent_project_process_reference.html`
- `preview.html`

---

## Task 1: Add Typed Message Dictionaries

**Files:**

- Create: `apps/desktop/src/i18n/messages.ts`
- Create: `apps/desktop/src/i18n/messages.test.ts`

- [ ] **Step 1: Create the failing message tests**

Add this file:

```ts
// apps/desktop/src/i18n/messages.test.ts
import { describe, expect, it } from "vitest";
import {
  defaultLocale,
  formatMessage,
  getMessage,
  isLocale,
  localeLabels,
  locales,
  messages,
} from "./messages";

describe("messages", () => {
  it("uses zh-CN as the default locale", () => {
    expect(defaultLocale).toBe("zh-CN");
  });

  it("accepts only supported locales", () => {
    expect(isLocale("zh-CN")).toBe(true);
    expect(isLocale("en-US")).toBe(true);
    expect(isLocale("fr-FR")).toBe(false);
    expect(isLocale(null)).toBe(false);
  });

  it("keeps visible language labels short", () => {
    expect(locales).toEqual(["zh-CN", "en-US"]);
    expect(localeLabels).toEqual({
      "zh-CN": "中文",
      "en-US": "EN",
    });
  });

  it("returns localized messages and falls back to English for missing keys", () => {
    expect(getMessage("zh-CN", "nav.skills")).toBe("技能");
    expect(getMessage("en-US", "nav.skills")).toBe("Skills");
    expect(getMessage("zh-CN", "missing.key")).toBe("missing.key");
  });

  it("interpolates named parameters", () => {
    expect(
      formatMessage("zh-CN", "actions.enablingSkillForTarget", {
        skillName: "agent-tool-safety",
        targetName: "Codex",
      }),
    ).toBe("正在为 Codex 启用 agent-tool-safety。");

    expect(
      formatMessage("en-US", "actions.enablingSkillForTarget", {
        skillName: "agent-tool-safety",
        targetName: "Codex",
      }),
    ).toBe("Enabling agent-tool-safety for Codex.");
  });

  it("keeps message keys aligned across locales", () => {
    expect(Object.keys(messages["zh-CN"]).sort()).toEqual(Object.keys(messages["en-US"]).sort());
  });
});
```

- [ ] **Step 2: Run the message tests and verify they fail**

Run:

```powershell
cd apps/desktop
npm test -- --run src/i18n/messages.test.ts
```

Expected: FAIL because `src/i18n/messages.ts` does not exist.

- [ ] **Step 3: Add the message dictionary implementation**

Create this file:

```ts
// apps/desktop/src/i18n/messages.ts
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
```

- [ ] **Step 4: Run the message tests and verify they pass**

Run:

```powershell
cd apps/desktop
npm test -- --run src/i18n/messages.test.ts
```

Expected: PASS for all tests in `messages.test.ts`.

- [ ] **Step 5: Commit Task 1**

Run:

```powershell
git add apps/desktop/src/i18n/messages.ts apps/desktop/src/i18n/messages.test.ts
git commit -m "feat: add bilingual message catalog"
```

Expected: commit succeeds and includes only the two i18n message files.

---

## Task 2: Add Locale Persistence And Language Switch

**Files:**

- Create: `apps/desktop/src/i18n/useLocale.ts`
- Create: `apps/desktop/src/i18n/useLocale.test.tsx`
- Create: `apps/desktop/src/components/LanguageSwitch.tsx`
- Create: `apps/desktop/src/components/LanguageSwitch.test.tsx`

- [ ] **Step 1: Write failing hook tests**

Add this file:

```tsx
// apps/desktop/src/i18n/useLocale.test.tsx
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { defaultLocale } from "./messages";
import { localeStorageKey, readStoredLocale, useLocale } from "./useLocale";

describe("useLocale", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("uses the default locale when storage is empty", () => {
    const { result } = renderHook(() => useLocale());

    expect(result.current.locale).toBe(defaultLocale);
    expect(result.current.t("nav.skills")).toBe("技能");
  });

  it("restores a valid stored locale", () => {
    localStorage.setItem(localeStorageKey, "en-US");

    const { result } = renderHook(() => useLocale());

    expect(result.current.locale).toBe("en-US");
    expect(result.current.t("nav.skills")).toBe("Skills");
  });

  it("falls back when storage contains an unsupported locale", () => {
    localStorage.setItem(localeStorageKey, "fr-FR");

    expect(readStoredLocale()).toBe(defaultLocale);
  });

  it("updates state and persists the next locale", () => {
    const { result } = renderHook(() => useLocale());

    act(() => {
      result.current.setLocale("en-US");
    });

    expect(result.current.locale).toBe("en-US");
    expect(localStorage.getItem(localeStorageKey)).toBe("en-US");
  });
});
```

- [ ] **Step 2: Write failing component tests**

Add this file:

```tsx
// apps/desktop/src/components/LanguageSwitch.test.tsx
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { LanguageSwitch } from "./LanguageSwitch";

describe("LanguageSwitch", () => {
  it("marks the active locale as pressed", () => {
    render(<LanguageSwitch locale="zh-CN" onLocaleChange={vi.fn()} />);

    expect(screen.getByRole("button", { name: "中文" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "EN" })).toHaveAttribute("aria-pressed", "false");
  });

  it("requests locale changes", () => {
    const onLocaleChange = vi.fn();
    render(<LanguageSwitch locale="zh-CN" onLocaleChange={onLocaleChange} />);

    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    expect(onLocaleChange).toHaveBeenCalledWith("en-US");
  });
});
```

- [ ] **Step 3: Run the new tests and verify they fail**

Run:

```powershell
cd apps/desktop
npm test -- --run src/i18n/useLocale.test.tsx src/components/LanguageSwitch.test.tsx
```

Expected: FAIL because `useLocale.ts` and `LanguageSwitch.tsx` do not exist.

- [ ] **Step 4: Add the locale hook**

Create this file:

```ts
// apps/desktop/src/i18n/useLocale.ts
import { useCallback, useMemo, useState } from "react";
import { defaultLocale, formatMessage, isLocale, type Locale, type MessageParams } from "./messages";

export const localeStorageKey = "skills-manage.locale";

export function readStoredLocale(storage: Storage = window.localStorage): Locale {
  const storedLocale = storage.getItem(localeStorageKey);
  return isLocale(storedLocale) ? storedLocale : defaultLocale;
}

export function writeStoredLocale(locale: Locale, storage: Storage = window.localStorage) {
  storage.setItem(localeStorageKey, locale);
}

export function useLocale() {
  const [locale, setLocaleState] = useState<Locale>(() => readStoredLocale());

  const setLocale = useCallback((nextLocale: Locale) => {
    setLocaleState(nextLocale);
    writeStoredLocale(nextLocale);
  }, []);

  const t = useCallback(
    (key: string, params?: MessageParams) => formatMessage(locale, key, params),
    [locale],
  );

  return useMemo(
    () => ({
      locale,
      setLocale,
      t,
    }),
    [locale, setLocale, t],
  );
}
```

- [ ] **Step 5: Add the language switch component**

Create this file:

```tsx
// apps/desktop/src/components/LanguageSwitch.tsx
import { localeLabels, locales, type Locale } from "../i18n/messages";

export function LanguageSwitch({
  locale,
  onLocaleChange,
}: {
  locale: Locale;
  onLocaleChange: (locale: Locale) => void;
}) {
  return (
    <div className="language-switch" aria-label="Language">
      {locales.map((item) => (
        <button
          aria-pressed={locale === item}
          className={locale === item ? "active" : ""}
          key={item}
          onClick={() => onLocaleChange(item)}
          type="button"
        >
          {localeLabels[item]}
        </button>
      ))}
    </div>
  );
}
```

- [ ] **Step 6: Run the locale and switch tests**

Run:

```powershell
cd apps/desktop
npm test -- --run src/i18n/useLocale.test.tsx src/components/LanguageSwitch.test.tsx
```

Expected: PASS for hook and switch tests.

- [ ] **Step 7: Commit Task 2**

Run:

```powershell
git add apps/desktop/src/i18n/useLocale.ts apps/desktop/src/i18n/useLocale.test.tsx apps/desktop/src/components/LanguageSwitch.tsx apps/desktop/src/components/LanguageSwitch.test.tsx
git commit -m "feat: persist ui language preference"
```

Expected: commit succeeds and includes only locale persistence and language switch files.

---

## Task 3: Wire Localized Text Into The App

**Files:**

- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.test.tsx`

- [ ] **Step 1: Update app tests for Chinese default and English switching**

In `apps/desktop/src/App.test.tsx`, update the first render test to clear storage and assert Chinese default UI:

```tsx
beforeEach(() => {
  invokeMock.mockReset();
  localStorage.clear();
});
```

Replace the first test body with:

```tsx
it("renders scanned backend skills with Chinese UI by default", async () => {
  invokeMock.mockResolvedValue([scannedCodexSkill]);

  render(<App />);

  expect(screen.getByRole("heading", { name: /Skills Manage/i })).toBeInTheDocument();
  expect(screen.getByRole("searchbox", { name: /搜索技能/i })).toBeInTheDocument();
  expect(screen.getByText("预览安全模式")).toBeInTheDocument();
  expect(screen.getByText("正在扫描本地文件夹")).toBeInTheDocument();

  await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
  expect(invokeMock).toHaveBeenCalledWith("scan_skills");
});
```

Add a new test after it:

```tsx
it("switches the visible app chrome to English without changing skill data", async () => {
  invokeMock.mockResolvedValue([scannedCodexSkill]);

  render(<App />);
  await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

  fireEvent.click(screen.getByRole("button", { name: "EN" }));

  expect(screen.getByRole("searchbox", { name: /Search skills/i })).toBeInTheDocument();
  expect(screen.getByText("Preview safe mode")).toBeInTheDocument();
  expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0);
  expect(localStorage.getItem("skills-manage.locale")).toBe("en-US");
});
```

Update the import assertion to expect Chinese default status:

```tsx
await waitFor(() => expect(screen.getByText(/已将 local-scan-skill 导入共享库/i)).toBeInTheDocument());
```

Update the target test click and status assertions:

```tsx
fireEvent.click(screen.getByRole("button", { name: "启用 Codex" }));
```

```tsx
await waitFor(() => expect(screen.getByText(/已为 Codex 启用 local-scan-skill/i)).toBeInTheDocument());
expect(screen.getByRole("button", { name: "停用 Codex" })).toBeInTheDocument();
```

- [ ] **Step 2: Run App tests and verify they fail**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: FAIL because `App.tsx` still renders English hard-coded UI and has no language switch.

- [ ] **Step 3: Import locale helpers and LanguageSwitch in App**

At the top of `apps/desktop/src/App.tsx`, add:

```tsx
import { LanguageSwitch } from "./components/LanguageSwitch";
import { useLocale } from "./i18n/useLocale";
```

Replace the current `healthLabels` constant with a function inside `App` or a helper:

```tsx
const healthLabelKeys: Record<SkillHealth, string> = {
  healthy: "health.healthy",
  warning: "health.warning",
  broken: "health.broken",
};
```

Replace `sections` with message keys:

```tsx
const sections = [
  { key: "nav.skills", icon: Boxes, active: true },
  { key: "nav.import", icon: FolderInput, active: false },
  { key: "nav.packages", icon: PackageOpen, active: false },
  { key: "nav.settings", icon: Settings, active: false },
];
```

- [ ] **Step 4: Wire locale state into App**

Inside `function App()`, add:

```tsx
const { locale, setLocale, t } = useLocale();
```

Use `t(...)` for dynamic messages:

```tsx
setScanError(error instanceof Error ? error.message : t("errors.scanFallback"));
```

```tsx
setTargetActionMessage(
  t(nextEnabled ? "actions.enablingSkillForTarget" : "actions.disablingSkillForTarget", {
    skillName: selectedSkill.name,
    targetName: target.name,
  }),
);
```

```tsx
setTargetActionMessage(
  t(result.enabled ? "actions.enabledSkillForTarget" : "actions.disabledSkillForTarget", {
    skillName: result.skillName,
    targetName: result.targetName,
  }),
);
```

```tsx
: t("errors.targetFallback", { targetName: target.name }),
```

```tsx
setImportMessage(t("actions.importingSkill", { skillName: selectedSkill.name }));
```

```tsx
setImportMessage(
  result.imported
    ? t("actions.importedSkill", { skillName: result.skillName })
    : result.message || t("actions.skillAlreadyManaged", { skillName: result.skillName }),
);
```

```tsx
: t("errors.importFallback"),
```

- [ ] **Step 5: Replace shell text with localized text**

In the returned JSX, replace the current rail/sidebar structure with a top control bar pattern:

```tsx
<main className="app-shell">
  <header className="topbar" aria-label="Application controls">
    <div className="window-dots" aria-hidden="true">
      <span />
      <span />
      <span />
    </div>
    <div className="brand-block">
      <strong>{t("app.title")}</strong>
      <span>{t("app.subtitle")}</span>
    </div>
    <nav className="module-tabs" aria-label="Primary navigation">
      {sections.map((section) => {
        const Icon = section.icon;
        const label = t(section.key);
        return (
          <button
            className={`module-tab ${section.active ? "active" : ""}`}
            type="button"
            key={section.key}
            aria-current={section.active ? "page" : undefined}
            aria-label={label}
            disabled={!section.active}
          >
            <Icon size={18} strokeWidth={1.8} />
            <span>{label}</span>
          </button>
        );
      })}
    </nav>
    <div className="topbar-actions">
      <LanguageSwitch locale={locale} onLocaleChange={setLocale} />
      <button className="topbar-icon-button" type="button" aria-label={t("app.workflowSettings")} disabled>
        <SlidersHorizontal size={18} strokeWidth={1.8} />
      </button>
    </div>
  </header>
```

Replace visible strings in the library pane:

```tsx
<p className="eyebrow">{t("app.subtitle")}</p>
<h1>{t("app.title")}</h1>
```

```tsx
<span className="mode-pill">
  <ShieldCheck size={16} strokeWidth={1.8} />
  {t("app.safeMode")}
</span>
```

```tsx
<SummaryStat label={t("stats.skills")} value={stats.total} />
<SummaryStat label={t("stats.healthy")} value={stats.healthy} />
<SummaryStat label={t("stats.review")} value={stats.warnings} />
<SummaryStat label={t("stats.enabledLinks")} value={stats.enabledTargets} />
```

```tsx
<span className="sr-only">{t("search.label")}</span>
<input
  aria-label={t("search.label")}
  type="search"
  placeholder={t("search.placeholder")}
  value={query}
  onChange={(event) => setQuery(event.currentTarget.value)}
/>
```

```tsx
aria-label={t("actions.scan")}
```

Replace scan status:

```tsx
{scanState === "scanning" ? (
  <span>{t("status.scan.scanning")}</span>
) : scanState === "error" ? (
  <span>{t("status.scan.error", { error: scanError })}</span>
) : usingFallback ? (
  <span>{t("status.scan.fallback")}</span>
) : (
  <span>{t("status.scan.ready")}</span>
)}
```

Replace empty state:

```tsx
<div className="empty-state">
  <p>{t("status.empty.title")}</p>
  <span>{t("status.empty.body")}</span>
</div>
```

- [ ] **Step 6: Pass translated labels into child render helpers**

Update `SkillListItem` signature:

```tsx
function SkillListItem({
  skill,
  selected,
  onSelect,
  targetCountLabel,
}: {
  skill: SkillRecord;
  selected: boolean;
  onSelect: () => void;
  targetCountLabel: string;
}) {
```

Call it with:

```tsx
<SkillListItem
  key={skill.id}
  skill={skill}
  selected={skill.id === selectedSkill?.id}
  onSelect={() => setSelectedId(skill.id)}
  targetCountLabel={t("detail.targets")}
/>
```

Inside `SkillListItem`, replace:

```tsx
<span>{enabledCount} targets</span>
```

with:

```tsx
<span>
  {enabledCount} {targetCountLabel}
</span>
```

Update `SkillDetail` props to receive:

```tsx
t: ReturnType<typeof useLocale>["t"];
```

Pass it from App:

```tsx
t={t}
```

Inside `SkillDetail`, replace all fixed labels with `t(...)`, including health badge:

```tsx
{t(healthLabelKeys[skill.health])}
```

Use localized action label:

```tsx
const importLabel =
  skill.source === "Shared Library"
    ? t("actions.alreadyInLibrary")
    : importState === "importing"
      ? t("actions.importing")
      : t("actions.importToLibrary");
```

Use localized target action labels:

```tsx
const actionVerb = target.enabled ? t("actions.disable") : t("actions.enable");
const actionLabel = `${actionVerb} ${target.name}`;
```

Use localized target status:

```tsx
<small>
  {skill.source === "Shared Library"
    ? target.enabled
      ? t("targets.enabled")
      : t("targets.disabled")
    : t("targets.unavailable")}
</small>
```

Replace footer strings:

```tsx
<span>{t("footer.dataRoot")}</span>
<span>{t("footer.backupMode")}</span>
<span>{t("footer.packageFormat")}</span>
```

- [ ] **Step 7: Run App tests**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: PASS for all App tests.

- [ ] **Step 8: Run all frontend tests**

Run:

```powershell
cd apps/desktop
npm test -- --run
```

Expected: PASS for all frontend tests.

- [ ] **Step 9: Commit Task 3**

Run:

```powershell
git add apps/desktop/src/App.tsx apps/desktop/src/App.test.tsx
git commit -m "feat: localize skills app chrome"
```

Expected: commit succeeds and includes only `App.tsx` and `App.test.tsx`.

---

## Task 4: Refresh The Layout Toward CC Switch

**Files:**

- Modify: `apps/desktop/src/App.css`
- Modify: `apps/desktop/src/App.test.tsx`

- [ ] **Step 1: Add one structure assertion before CSS work**

Add this assertion to the default render test after `render(<App />);`:

```tsx
expect(screen.getByRole("banner", { name: /Application controls/i })).toBeInTheDocument();
expect(screen.getByRole("navigation", { name: /Primary navigation/i })).toBeInTheDocument();
```

- [ ] **Step 2: Run App tests**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: PASS because Task 3 already introduced the top control bar.

- [ ] **Step 3: Replace shell CSS with compact light control-surface styles**

In `apps/desktop/src/App.css`, keep the reset, root tokens, focus rules, and responsive sections. Replace layout-specific blocks for `.app-shell`, `.rail`, `.rail-*`, `.library-pane`, `.detail-pane`, rows, buttons, and footer with this direction:

```css
:root {
  font-family:
    "Segoe UI",
    system-ui,
    -apple-system,
    BlinkMacSystemFont,
    sans-serif;
  color: #182026;
  background: #f4f5f6;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
  --surface: #f6f7f8;
  --surface-strong: #ffffff;
  --surface-raised: #fbfcfd;
  --surface-muted: #eef0f2;
  --line: #dde2e6;
  --line-strong: #c7d0d7;
  --text: #182026;
  --muted: #66727c;
  --accent: #2f7df6;
  --accent-strong: #1f63d5;
  --success: #168251;
  --warning: #a15d00;
  --danger: #b42335;
  --shadow-sm: 0 1px 2px rgba(21, 32, 43, 0.08);
  --shadow-row: 0 8px 24px rgba(21, 32, 43, 0.08);
}

.app-shell {
  display: grid;
  grid-template-columns: minmax(360px, 460px) minmax(520px, 1fr);
  grid-template-rows: 76px 1fr 36px;
  width: 100vw;
  min-height: 100vh;
  color: var(--text);
  background: var(--surface);
}

.topbar {
  grid-column: 1 / -1;
  display: grid;
  grid-template-columns: 62px minmax(160px, 240px) minmax(300px, 1fr) auto;
  gap: 16px;
  align-items: center;
  padding: 14px 24px;
  border-bottom: 1px solid var(--line);
  background: rgba(255, 255, 255, 0.94);
}

.window-dots {
  display: flex;
  gap: 8px;
}

.window-dots span {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: #d8dde2;
}

.window-dots span:nth-child(1) {
  background: #ff6159;
}

.window-dots span:nth-child(2) {
  background: #ffbd2d;
}

.window-dots span:nth-child(3) {
  background: #28c840;
}

.brand-block {
  display: grid;
  gap: 2px;
  min-width: 0;
}

.brand-block strong {
  color: var(--accent);
  font-size: 20px;
  line-height: 1.1;
}

.brand-block span {
  overflow: hidden;
  color: var(--muted);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.module-tabs {
  display: inline-flex;
  justify-self: center;
  min-width: 0;
  padding: 4px;
  border-radius: 8px;
  background: var(--surface-muted);
}

.module-tab {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-height: 38px;
  padding: 0 14px;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--muted);
  font-weight: 700;
  cursor: pointer;
}

.module-tab.active {
  background: var(--surface-strong);
  color: var(--text);
  box-shadow: var(--shadow-sm);
}

.module-tab:disabled {
  cursor: not-allowed;
  opacity: 0.72;
}

.topbar-actions {
  display: inline-flex;
  justify-content: end;
  gap: 10px;
}

.language-switch {
  display: inline-flex;
  min-height: 38px;
  padding: 4px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-strong);
}

.language-switch button {
  min-height: 30px;
  padding: 0 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--muted);
  font-weight: 800;
  cursor: pointer;
}

.language-switch button.active {
  background: var(--accent);
  color: #ffffff;
}

.topbar-icon-button,
.icon-action {
  display: grid;
  width: 38px;
  min-height: 38px;
  place-items: center;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-strong);
  color: var(--muted);
}

.library-pane {
  min-width: 0;
  padding: 18px 18px 16px 24px;
  border-right: 1px solid var(--line);
  background: var(--surface);
}

.detail-pane {
  min-width: 0;
  padding: 18px 24px 52px;
  background: #f8f9fa;
}

.skills-list {
  display: grid;
  gap: 10px;
  margin-top: 14px;
}

.skill-row {
  display: grid;
  grid-template-columns: 30px minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  width: 100%;
  min-height: 76px;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-strong);
  color: var(--text);
  text-align: left;
  cursor: pointer;
  box-shadow: var(--shadow-sm);
}

.skill-row:hover {
  border-color: var(--line-strong);
  box-shadow: var(--shadow-row);
}

.skill-row.selected {
  border-color: rgba(47, 125, 246, 0.62);
  box-shadow:
    inset 3px 0 0 var(--accent),
    var(--shadow-row);
}

.detail-section {
  padding: 14px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-strong);
  box-shadow: var(--shadow-sm);
}

.target-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 104px;
  gap: 12px;
  align-items: center;
  padding: 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-raised);
}

.target-row button:not(:disabled) {
  border-color: rgba(47, 125, 246, 0.36);
  background: #eaf2ff;
  color: var(--accent-strong);
  cursor: pointer;
}

.action-bar button.primary-action {
  border-color: rgba(47, 125, 246, 0.5);
  background: var(--accent);
  color: #ffffff;
}

.status-strip {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  gap: 18px;
  min-width: 0;
  padding: 0 18px;
  border-top: 1px solid var(--line);
  background: #ffffff;
  color: var(--muted);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
}
```

Keep existing supporting CSS for `.sr-only`, `.pane-header`, `.stat-grid`, `.summary-stat`, `.toolbar`, `.search-field`, `.scan-status`, `.health-dot`, `.health-badge`, `.metadata-grid`, `.action-bar`, `.import-status`, `.target-action-status`, and responsive breakpoints, but adjust class names that referenced `.rail`.

- [ ] **Step 4: Update responsive CSS**

Replace the old `@media (max-width: 980px)` and `@media (max-width: 720px)` layout sections with:

```css
@media (max-width: 980px) {
  .app-shell {
    grid-template-columns: 1fr;
    grid-template-rows: auto auto auto 36px;
  }

  .topbar {
    grid-template-columns: 1fr;
    gap: 12px;
    align-items: stretch;
  }

  .window-dots {
    display: none;
  }

  .module-tabs {
    justify-self: stretch;
    overflow-x: auto;
  }

  .topbar-actions {
    justify-content: space-between;
  }

  .library-pane,
  .detail-pane {
    padding: 16px;
  }

  .detail-pane {
    grid-column: 1;
  }
}

@media (max-width: 720px) {
  .stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .skill-row {
    grid-template-columns: 30px minmax(0, 1fr);
  }

  .skill-row-meta {
    grid-column: 2;
    grid-auto-flow: column;
    justify-content: start;
    text-align: left;
  }

  .metadata-grid div,
  .target-row {
    grid-template-columns: 1fr;
  }

  .action-bar {
    display: grid;
  }
}
```

- [ ] **Step 5: Run all frontend tests**

Run:

```powershell
cd apps/desktop
npm test -- --run
```

Expected: PASS for all frontend tests.

- [ ] **Step 6: Run frontend build**

Run:

```powershell
cd apps/desktop
npm run build
```

Expected: PASS with Vite producing `dist`.

- [ ] **Step 7: Commit Task 4**

Run:

```powershell
git add apps/desktop/src/App.css apps/desktop/src/App.test.tsx
git commit -m "style: refresh compact bilingual app shell"
```

Expected: commit succeeds and includes CSS plus any test assertion update.

---

## Task 5: Browser QA For Chinese And English States

**Files:**

- Modify only if QA finds a visible issue: `apps/desktop/src/App.css` or `apps/desktop/src/App.tsx`

- [ ] **Step 1: Start the frontend preview or dev target**

Use the existing local development setup:

```powershell
cd apps/desktop
npm run dev -- --host 127.0.0.1
```

Expected: Vite prints a local URL such as `http://127.0.0.1:5173/`.

- [ ] **Step 2: Open the local URL in the in-app browser**

Use the Browser plugin to open the URL from Step 1.

Expected: the app renders with Chinese labels by default.

- [ ] **Step 3: Inspect the Chinese default screen**

Check these visible points:

- The top control bar is light and compact.
- `中文` is selected in the language switch.
- Search label, scan status, safe-mode pill, stat labels, metadata labels, target labels, action buttons, and footer labels are Chinese.
- Skill names and descriptions remain unchanged source data.
- No row, button, target control, or footer text overlaps.

- [ ] **Step 4: Switch to English and inspect**

Click `EN`.

Expected:

- `EN` is selected.
- UI chrome changes to English.
- Skill names and descriptions remain unchanged.
- No row, button, target control, or footer text overlaps.

- [ ] **Step 5: Run the final frontend checks**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
```

Expected: both commands PASS.

- [ ] **Step 6: Commit QA fixes if any were needed**

If CSS or TSX changed during QA, run:

```powershell
git add apps/desktop/src/App.css apps/desktop/src/App.tsx apps/desktop/src/App.test.tsx
git commit -m "fix: polish bilingual ui layout"
```

Expected: commit succeeds only when QA produced code changes. If QA found no issue, do not create an empty commit.

---

## Task 6: Final Review And Handoff

**Files:**

- Read: `docs/superpowers/specs/2026-06-07-bilingual-ccswitch-ui-design.md`
- Read: `docs/superpowers/plans/2026-06-07-bilingual-ccswitch-ui.md`
- Read: `git diff main...HEAD`

- [ ] **Step 1: Verify spec coverage**

Use this checklist:

- Chinese and English UI chrome exist.
- Chinese is the default locale.
- Language switching is immediate.
- Locale persists in localStorage.
- Skill data is not translated.
- Existing Tauri scan/import/target commands are unchanged.
- UI has compact CC Switch-inspired top controls and list-first layout.
- Accessibility labels and status roles are preserved.
- Frontend tests cover locale fallback and app switching.
- Build passes.

Expected: every item is covered by code or tests from Tasks 1-5.

- [ ] **Step 2: Review the branch diff**

Run:

```powershell
git diff --stat main...HEAD
git diff --name-only main...HEAD
```

Expected changed files are limited to:

```text
apps/desktop/src/App.css
apps/desktop/src/App.test.tsx
apps/desktop/src/App.tsx
apps/desktop/src/components/LanguageSwitch.test.tsx
apps/desktop/src/components/LanguageSwitch.tsx
apps/desktop/src/i18n/messages.test.ts
apps/desktop/src/i18n/messages.ts
apps/desktop/src/i18n/useLocale.test.tsx
apps/desktop/src/i18n/useLocale.ts
docs/superpowers/plans/2026-06-07-bilingual-ccswitch-ui.md
docs/superpowers/specs/2026-06-07-bilingual-ccswitch-ui-design.md
```

- [ ] **Step 3: Run final verification**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
```

Expected: both commands PASS.

- [ ] **Step 4: Report risks**

Include these notes in the handoff:

- Locale persistence is currently renderer localStorage and should later move to `settings.json`.
- Raw Rust filesystem errors may remain English until backend error normalization.
- This phase does not implement new Settings, Packages, Import, or backend storage screens.
- Ignored local HTML references remain untracked.

Expected: handoff is clear enough for the next PR description.

---

## Self-Review Notes

Spec coverage:

- Bilingual default and switching are covered by Tasks 1-3.
- Persistence is covered by Task 2.
- CC Switch-inspired compact layout is covered by Task 4.
- Browser visual checks are covered by Task 5.
- Safety boundaries and no backend changes are covered by the file map and Task 6 diff check.

Placeholder scan:

- The plan contains no open requirement placeholders.
- Every task has files, commands, expected outcomes, and commit scope.

Type consistency:

- Locale type is `Locale`.
- Locale values are `zh-CN` and `en-US`.
- Storage key is `skills-manage.locale`.
- Translation helper signature is `t(key, params?)`.
