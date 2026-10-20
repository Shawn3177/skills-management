import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const appStyles = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "App.css"), "utf8");

function cssBlock(selectorPattern) {
  const match = selectorPattern.exec(appStyles);
  expect(match).not.toBeNull();
  return match?.[1] ?? "";
}

function customHexProperty(name) {
  const match = new RegExp(`${name}:\\s*(#[0-9a-fA-F]{6})`).exec(appStyles);
  expect(match).not.toBeNull();
  return match?.[1] ?? "#000000";
}

function relativeLuminance(hexColor) {
  const [red, green, blue] = hexColor
    .replace("#", "")
    .match(/.{2}/g)
    .map((channel) => {
      const value = Number.parseInt(channel, 16) / 255;
      return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    });

  return red * 0.2126 + green * 0.7152 + blue * 0.0722;
}

function contrastRatio(foreground, background) {
  const lighter = Math.max(relativeLuminance(foreground), relativeLuminance(background));
  const darker = Math.min(relativeLuminance(foreground), relativeLuminance(background));

  return (lighter + 0.05) / (darker + 0.05);
}

describe("App stylesheet contracts", () => {
  it("keeps the app as a fixed desktop window with one scrolling content area", () => {
    expect(cssBlock(/body\s*\{([^}]*)\}/s)).toContain("overflow: hidden");
    expect(cssBlock(/\.app\s*\{([^}]*)\}/s)).toContain("height: 100vh");
    expect(cssBlock(/\.app\s*\{([^}]*)\}/s)).toContain("overflow: hidden");
    expect(cssBlock(/\.scroll-surface\s*\{([^}]*)\}/s)).toContain("overflow-y: auto");
  });

  it("keeps desktop pointer targets comfortably clickable", () => {
    expect(appStyles).toContain("--control: 32px");
    expect(cssBlock(/\.button\s*\{([^}]*)\}/s)).toContain("min-height: var(--control)");
    expect(cssBlock(/\.tool-tabs button\s*\{([^}]*)\}/s)).toContain("min-height: 32px");
    expect(cssBlock(/\.fab\s*\{([^}]*)\}/s)).toContain("width: 36px");
    expect(cssBlock(/\.back-button\s*\{([^}]*)\}/s)).toContain("width: 34px");
  });

  it("keeps a visible keyboard focus ring and reveals hover actions on focus", () => {
    expect(cssBlock(/:focus-visible\s*\{([^}]*)\}/s)).toContain("outline: 2px solid var(--accent)");
    expect(appStyles).toContain(".skill-card:focus-within .hover-actions");
  });

  it("keeps white text readable on the accent and danger fills", () => {
    expect(contrastRatio("#ffffff", customHexProperty("--accent"))).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio("#ffffff", customHexProperty("--danger-fill"))).toBeGreaterThanOrEqual(4.5);
  });
});
