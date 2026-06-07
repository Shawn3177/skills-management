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
