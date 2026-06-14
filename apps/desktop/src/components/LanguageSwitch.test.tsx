import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LanguageSwitch } from "./LanguageSwitch";

describe("LanguageSwitch", () => {
  afterEach(() => {
    cleanup();
  });

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
