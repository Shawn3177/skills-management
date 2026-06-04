import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invokeMock = vi.fn();

const scannedCodexSkill = {
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
};

const sharedLibrarySkill = {
  ...scannedCodexSkill,
  source: "Shared Library",
  sourcePath: "C:/Users/example/.skills-manage/library/local-scan-skill",
  targets: scannedCodexSkill.targets.map((target) => ({ ...target, enabled: false })),
};

const sharedLibrarySkillWithCodexEnabled = {
  ...sharedLibrarySkill,
  targets: sharedLibrarySkill.targets.map((target) =>
    target.id === "codex" ? { ...target, enabled: true } : target,
  ),
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders scanned backend skills when scan_skills returns records", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);

    expect(screen.getByRole("heading", { name: /Skills Manage/i })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: /Search skills/i })).toBeInTheDocument();
    expect(screen.getByText("Preview safe mode")).toBeInTheDocument();
    expect(screen.getByText("Scanning local folders")).toBeInTheDocument();

    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    expect(invokeMock).toHaveBeenCalledWith("scan_skills");
  });

  it("imports the selected skill into the shared library and refreshes the scan", async () => {
    let scanCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(scanCount === 1 ? [scannedCodexSkill] : [sharedLibrarySkill]);
      }

      if (command === "import_skill_to_library") {
        return Promise.resolve({
          imported: true,
          alreadyManaged: false,
          skillName: "local-scan-skill",
          libraryPath: "C:/Users/example/.skills-manage/library/local-scan-skill",
          message: "Skill imported into the shared library.",
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole("button", { name: /Import to library/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_skill_to_library", {
        sourcePath: scannedCodexSkill.sourcePath,
      }),
    );
    await waitFor(() => expect(screen.getByText(/Imported local-scan-skill/i)).toBeInTheDocument());
    expect(screen.getAllByText("Shared Library").length).toBeGreaterThan(0);
  });

  it("enables the selected shared-library skill for a target and refreshes the scan", async () => {
    let scanCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(scanCount === 1 ? [sharedLibrarySkill] : [sharedLibrarySkillWithCodexEnabled]);
      }

      if (command === "set_skill_target_enabled") {
        return Promise.resolve({
          targetId: "codex",
          targetName: "Codex",
          skillName: "local-scan-skill",
          enabled: true,
          changed: true,
          targetPath: "C:/Users/example/.codex/skills/local-scan-skill",
          message: "Enabled local-scan-skill for Codex.",
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole("button", { name: "Enable Codex" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_skill_target_enabled", {
        sourcePath: sharedLibrarySkill.sourcePath,
        targetId: "codex",
        enabled: true,
      }),
    );
    await waitFor(() => expect(screen.getByText(/Enabled local-scan-skill for Codex/i)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Disable Codex" })).toBeInTheDocument();
  });
});
