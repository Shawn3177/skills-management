import "@testing-library/jest-dom/vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders scanned backend skills when scan_skills returns records", async () => {
    invokeMock.mockResolvedValue([
      {
        id: "local-scan-skill",
        name: "local-scan-skill",
        description: "Loaded from a local SKILL.md file.",
        source: "Codex",
        sourcePath: "C:/Users/example/.codex/skills/local-scan-skill",
        health: "healthy",
        targets: [
          { id: "codex", name: "Codex", enabled: true },
          { id: "claude-code", name: "Claude Code", enabled: false },
          { id: "vs-code", name: "VS Code", enabled: false },
        ],
        supportFiles: ["SKILL.md"],
      },
    ]);

    render(<App />);

    expect(screen.getByRole("heading", { name: /Skills Manage/i })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: /Search skills/i })).toBeInTheDocument();
    expect(screen.getByText("Preview safe mode")).toBeInTheDocument();
    expect(screen.getByText("Scanning local folders")).toBeInTheDocument();

    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    expect(invokeMock).toHaveBeenCalledWith("scan_skills");
  });
});
