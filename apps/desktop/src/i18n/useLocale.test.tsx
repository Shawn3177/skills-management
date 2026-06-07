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
