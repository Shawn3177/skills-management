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

describe("App stylesheet accessibility contracts", () => {
  it("keeps the app as a fixed desktop shell instead of a scrolling page", () => {
    expect(cssBlock(/body\s*\{([^}]*)\}/s)).toContain("overflow: hidden");
    expect(cssBlock(/\.app-shell\s*\{([^}]*)\}/s)).toContain("height: 100vh");
    expect(cssBlock(/\.app-shell\s*\{([^}]*)\}/s)).toContain("overflow: hidden");
    expect(cssBlock(/\.app-shell\s*\{([^}]*)\}/s)).toContain("grid-template-columns: 64px minmax(0, 1fr)");
  });

  it("hides scrollbars while keeping internal scroll surfaces wheelable", () => {
    expect(cssBlock(/\.scroll-surface\s*\{([^}]*)\}/s)).toContain("overflow-y: auto");
    expect(cssBlock(/\.scroll-surface\s*\{([^}]*)\}/s)).toContain("scrollbar-width: none");
    expect(cssBlock(/\.scroll-surface::-webkit-scrollbar\s*\{([^}]*)\}/s)).toContain("display: none");
  });

  it("keeps compact exe controls at touch-safe sizes", () => {
    expect(cssBlock(/\.rail-button\s*\{([^}]*)\}/s)).toContain("width: 44px");
    expect(cssBlock(/\.rail-button\s*\{([^}]*)\}/s)).toContain("min-height: 44px");
    expect(cssBlock(/\.language-switch\s*\{([^}]*)\}/s)).toContain("min-height: 44px");
    expect(cssBlock(/\.language-switch button\s*\{([^}]*)\}/s)).toContain("min-height: 44px");
    expect(cssBlock(/\.icon-action\s*\{([^}]*)\}/s)).toContain("width: 44px");
    expect(cssBlock(/\.icon-action\s*\{([^}]*)\}/s)).toContain("min-height: 44px");
  });

  it("keeps keyboard focus visible for the search field", () => {
    expect(cssBlock(/\.search-field:focus-within\s*\{([^}]*)\}/s)).toContain("outline:");
    expect(cssBlock(/\.search-field:focus-within\s*\{([^}]*)\}/s)).toContain("outline-offset:");
  });

  it("keeps white text readable on the primary accent fill", () => {
    expect(contrastRatio("#ffffff", customHexProperty("--accent"))).toBeGreaterThanOrEqual(4.5);
  });
});
