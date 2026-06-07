import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { messages } from "./i18n/messages";

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
    localStorage.clear();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders scanned backend skills with Chinese UI by default", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);

    expect(screen.getByRole("complementary", { name: messages["zh-CN"]["regions.appControls"] })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: messages["zh-CN"]["regions.discoveredSkills"] })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: messages["zh-CN"]["regions.skillDetail"] })).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: messages["zh-CN"]["search.label"] })).toBeInTheDocument();
    expect(screen.getByText(messages["zh-CN"]["drawer.selectedSkill"])).toBeInTheDocument();
    expect(screen.getByText(messages["zh-CN"]["status.scan.scanning"])).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: messages["zh-CN"]["regions.librarySummary"] })).not.toBeInTheDocument();
    expect(screen.queryByText(messages["zh-CN"]["footer.dataRoot"])).not.toBeInTheDocument();

    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    expect(invokeMock).toHaveBeenCalledWith("scan_skills");
  });

  it("switches the visible app chrome to English without changing skill data", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    expect(screen.getByRole("searchbox", { name: /Search skills/i })).toBeInTheDocument();
    expect(screen.getByText("Selected skill")).toBeInTheDocument();
    expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0);
    expect(localStorage.getItem("skills-manage.locale")).toBe("en-US");
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

    fireEvent.click(screen.getByRole("button", { name: /导入共享库/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_skill_to_library", {
        sourcePath: scannedCodexSkill.sourcePath,
      }),
    );
    await waitFor(() => expect(screen.getByText(/已将 local-scan-skill 导入共享库/i)).toBeInTheDocument());
    expect(screen.getAllByText("Shared Library").length).toBeGreaterThan(0);
  });

  it("keeps already-managed import success messages localized", async () => {
    let scanCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(scanCount === 1 ? [scannedCodexSkill] : [sharedLibrarySkill]);
      }

      if (command === "import_skill_to_library") {
        return Promise.resolve({
          imported: false,
          alreadyManaged: true,
          skillName: "local-scan-skill",
          libraryPath: "C:/Users/example/.skills-manage/library/local-scan-skill",
          message: "Skill is already in the shared library.",
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole("button", { name: /导入共享库/i }));

    await waitFor(() => expect(screen.getByText("local-scan-skill 已在共享库中。")).toBeInTheDocument());
    expect(screen.queryByText("Skill is already in the shared library.")).not.toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "启用 Codex" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_skill_target_enabled", {
        sourcePath: sharedLibrarySkill.sourcePath,
        targetId: "codex",
        enabled: true,
      }),
    );
    await waitFor(() => expect(screen.getByText(/已为 Codex 启用 local-scan-skill/i)).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "停用 Codex" })).toBeInTheDocument();
  });
});
