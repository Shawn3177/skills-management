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

  it("returns localized messages and falls back to the key for missing messages", () => {
    expect(getMessage("zh-CN", "tabs.all")).toBe("全部");
    expect(getMessage("en-US", "tabs.all")).toBe("All");
    expect(getMessage("zh-CN", "missing.key")).toBe("missing.key");
  });

  it("interpolates named parameters", () => {
    expect(formatMessage("zh-CN", "actions.enableIn", { skill: "代码审查", tool: "Codex" })).toBe(
      "在 Codex 中启用 代码审查",
    );
    expect(formatMessage("en-US", "actions.enableIn", { skill: "review", tool: "Codex" })).toBe(
      "Enable review in Codex",
    );
    expect(formatMessage("en-US", "summary.tool", { total: 4, tool: "Codex" })).toBe(
      "4 skills in the shared library, {count} enabled in Codex",
    );
  });

  it("keeps message keys aligned across locales", () => {
    expect(Object.keys(messages["zh-CN"]).sort()).toEqual(Object.keys(messages["en-US"]).sort());
  });

  it("uses the same placeholders in both locales", () => {
    const placeholders = (text: string) => (text.match(/\{\w+\}/g) ?? []).sort();
    for (const key of Object.keys(messages["zh-CN"]) as Array<keyof (typeof messages)["zh-CN"]>) {
      expect(placeholders(messages["en-US"][key]), key).toEqual(placeholders(messages["zh-CN"][key]));
    }
  });
});
