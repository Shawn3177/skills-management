import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { messages } from "./i18n/messages";

const invokeMock = vi.fn();
const saveMock = vi.fn();
const openMock = vi.fn();

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

const duplicateClaudeSkill = {
  ...scannedCodexSkill,
  id: "local-scan-skill-claude",
  source: "Claude Code",
  sourcePath: "C:/Users/example/.claude/skills/local-scan-skill",
  targets: scannedCodexSkill.targets.map((target) =>
    target.id === "claude-code" ? { ...target, enabled: true } : { ...target, enabled: false },
  ),
};

const secondScannedSkill = {
  ...scannedCodexSkill,
  id: "second-local-skill",
  name: "second-local-skill",
  sourcePath: "C:/Users/example/.codex/skills/second-local-skill",
  description: "Another local skill waiting to be imported.",
};

const secondSharedLibrarySkill = {
  ...secondScannedSkill,
  source: "Shared Library",
  sourcePath: "C:/Users/example/.skills-manage/library/second-local-skill",
  targets: secondScannedSkill.targets.map((target) => ({ ...target, enabled: false })),
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveMock(...args),
  open: (...args: unknown[]) => openMock(...args),
}));

function openLocalScanSkill() {
  fireEvent.click(screen.getByRole("button", { name: /local-scan-skill/ }));
}

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    saveMock.mockReset();
    openMock.mockReset();
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
    expect(screen.queryByRole("region", { name: messages["zh-CN"]["regions.skillDetail"] })).not.toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: messages["zh-CN"]["search.label"] })).toBeInTheDocument();
    expect(screen.queryByText(messages["zh-CN"]["drawer.selectedSkill"])).not.toBeInTheDocument();
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
    expect(screen.queryByText("Selected skill")).not.toBeInTheDocument();
    expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0);
    expect(localStorage.getItem("skills-manage.locale")).toBe("en-US");
  });

  it("uses a calm browser-preview status instead of showing the raw Tauri bridge error", async () => {
    invokeMock.mockRejectedValue(new Error("Cannot read properties of undefined (reading 'invoke')"));

    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    await waitFor(() =>
      expect(screen.getByText("Desktop scan runs in the app window. Showing sample records.")).toBeInTheDocument(),
    );
    expect(screen.queryByText(/Cannot read properties of undefined/)).not.toBeInTheDocument();
    expect(screen.queryByText(/reading 'invoke'/)).not.toBeInTheDocument();
  });

  it("lets the rail navigation switch between available workspaces", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    const importNav = screen.getByRole("button", { name: "Imported" });
    expect(importNav).toBeEnabled();
    fireEvent.click(importNav);
    expect(screen.getByRole("region", { name: "Imported workspace" })).toBeInTheDocument();
    expect(importNav).toHaveAttribute("aria-current", "page");

    const packagesNav = screen.getByRole("button", { name: "Packages" });
    expect(packagesNav).toBeEnabled();
    fireEvent.click(packagesNav);
    expect(screen.getByRole("region", { name: "Packages workspace" })).toBeInTheDocument();

    const settingsNav = screen.getByRole("button", { name: "Settings" });
    expect(settingsNav).toBeEnabled();
    fireEvent.click(settingsNav);
    expect(screen.getByRole("region", { name: "Settings workspace" })).toBeInTheDocument();
  });

  it("lists imported skills with per-target switches and toggles them from the Imported tab", async () => {
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
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Imported" }));

    const codexSwitch = screen.getByRole("switch", { name: "Enable Codex" });
    expect(codexSwitch).toHaveAttribute("aria-checked", "false");

    fireEvent.click(codexSwitch);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_skill_target_enabled", {
        sourcePath: sharedLibrarySkill.sourcePath,
        targetId: "codex",
        enabled: true,
      }),
    );
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Disable Codex" })).toHaveAttribute("aria-checked", "true"),
    );
  });

  it("shows an empty state in the Imported tab when nothing has been imported", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Imported" }));

    expect(screen.getByText("No skills imported yet.")).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  it("enables a tool for every imported skill from the bulk action", async () => {
    let scanCount = 0;
    const withClaude = (skill: typeof sharedLibrarySkill) => ({
      ...skill,
      targets: skill.targets.map((target) =>
        target.id === "claude-code" ? { ...target, enabled: true } : target,
      ),
    });
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(
          scanCount === 1
            ? [sharedLibrarySkill, secondSharedLibrarySkill]
            : [withClaude(sharedLibrarySkill), withClaude(secondSharedLibrarySkill)],
        );
      }

      if (command === "set_skill_targets_bulk") {
        return Promise.resolve({
          succeeded: 2,
          failed: 0,
          targetId: "claude-code",
          targetName: "Claude Code",
          enabled: true,
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Imported" }));

    fireEvent.click(screen.getByRole("button", { name: "Enable all for Claude Code" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_skill_targets_bulk", {
        sourcePaths: [sharedLibrarySkill.sourcePath, secondSharedLibrarySkill.sourcePath],
        targetId: "claude-code",
        enabled: true,
      }),
    );
    await waitFor(() =>
      expect(screen.getByText("Enabled 2 skills for Claude Code.")).toBeInTheDocument(),
    );
  });

  it("surfaces the failed count when a bulk enable partially fails", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        return Promise.resolve([sharedLibrarySkill, secondSharedLibrarySkill]);
      }

      if (command === "set_skill_targets_bulk") {
        return Promise.resolve({
          succeeded: 1,
          failed: 1,
          targetId: "claude-code",
          targetName: "Claude Code",
          enabled: true,
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Imported" }));

    fireEvent.click(screen.getByRole("button", { name: "Enable all for Claude Code" }));

    await waitFor(() =>
      expect(screen.getByText("Enabled 1 skills for Claude Code, 1 failed.")).toBeInTheDocument(),
    );
  });

  it("disables the bulk enable button when every imported skill is already on for a tool", async () => {
    const codexOn = {
      ...sharedLibrarySkill,
      targets: sharedLibrarySkill.targets.map((target) =>
        target.id === "codex" ? { ...target, enabled: true } : target,
      ),
    };
    invokeMock.mockResolvedValue([codexOn]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Imported" }));

    expect(screen.getByRole("button", { name: "Enable all for Codex" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Disable all for Codex" })).toBeEnabled();
  });

  it("opens skill details only after selecting a skill row", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    expect(screen.queryByRole("region", { name: "Skill detail" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /local-scan-skill/ }));

    expect(screen.getByRole("region", { name: "Skill detail" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Back to skills" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Back to skills" }));
    expect(screen.queryByRole("region", { name: "Skill detail" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Discovered skills" })).toBeInTheDocument();
  });

  it("groups same-named skills into one list row with usage labels", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill, duplicateClaudeSkill, sharedLibrarySkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    const skillRows = screen.getAllByRole("button", { name: /local-scan-skill/ });
    expect(skillRows).toHaveLength(1);
    expect(within(skillRows[0]).getByText("Codex")).toBeInTheDocument();
    expect(within(skillRows[0]).getByText("Claude Code")).toBeInTheDocument();
    expect(within(skillRows[0]).queryByText("Shared Library")).not.toBeInTheDocument();
    expect(within(skillRows[0]).queryByText(/targets/i)).not.toBeInTheDocument();
    openLocalScanSkill();
    expect(screen.getByText("Codex, Claude Code, Shared Library")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Already in library" })).toBeDisabled();
  });

  it("shows settings labels without repeating their values", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByText("Data root")).toBeInTheDocument();
    expect(screen.getByText("Package format")).toBeInTheDocument();
    expect(screen.getAllByText("%USERPROFILE%\\.skills-manage")).toHaveLength(1);
    expect(screen.getAllByText(".skillpack")).toHaveLength(1);
  });

  it("maps a backend target conflict to a calm localized message", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        return Promise.resolve([sharedLibrarySkill]);
      }

      if (command === "set_skill_target_enabled") {
        return Promise.reject(
          new Error(
            "Target folder already exists and is not managed by Skills Manage: C:/Users/example/.codex/skills/local-scan-skill",
          ),
        );
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    openLocalScanSkill();

    fireEvent.click(screen.getByRole("button", { name: "启用 Codex" }));

    await waitFor(() =>
      expect(screen.getByText(messages["zh-CN"]["errors.targetConflict"])).toBeInTheDocument(),
    );
    expect(screen.queryByText(/Target folder already exists/)).not.toBeInTheDocument();
  });

  it("maps a backend import failure to a calm localized message", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        return Promise.resolve([scannedCodexSkill]);
      }

      if (command === "import_skill_to_library") {
        return Promise.reject(new Error("ENOSPC: no space left on device, write"));
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    openLocalScanSkill();

    fireEvent.click(screen.getByRole("button", { name: /导入共享库/i }));

    await waitFor(() =>
      expect(screen.getByText(messages["zh-CN"]["errors.importFallback"])).toBeInTheDocument(),
    );
    expect(screen.queryByText(/ENOSPC/)).not.toBeInTheDocument();
  });

  it("keeps repair as a preview and exports the selected skill to a .skillpack", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        return Promise.resolve([scannedCodexSkill]);
      }
      if (command === "export_skillpack") {
        return Promise.resolve({
          skillCount: 1,
          destination: "C:/out/local-scan-skill.skillpack",
          message: "ok",
        });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    openLocalScanSkill();

    fireEvent.click(screen.getByRole("button", { name: "Repair" }));
    expect(
      screen.getByText("Repair checks for local-scan-skill are waiting for the repair backend."),
    ).toBeInTheDocument();

    saveMock.mockResolvedValue("C:/out/local-scan-skill.skillpack");
    fireEvent.click(screen.getByRole("button", { name: "Export .skillpack" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_skillpack",
        expect.objectContaining({
          destination: "C:/out/local-scan-skill.skillpack",
          sources: expect.arrayContaining([
            expect.objectContaining({ sourcePath: scannedCodexSkill.sourcePath, name: "local-scan-skill" }),
          ]),
        }),
      ),
    );
    await waitFor(() => expect(screen.getByText("Exported 1 skills to .skillpack.")).toBeInTheDocument());
  });

  it("does not export when the save dialog is cancelled", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);
    saveMock.mockResolvedValue(null);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    openLocalScanSkill();

    fireEvent.click(screen.getByRole("button", { name: "Export .skillpack" }));

    await waitFor(() => expect(saveMock).toHaveBeenCalled());
    expect(invokeMock).not.toHaveBeenCalledWith("export_skillpack", expect.anything());
  });

  it("imports a .skillpack chosen from the Packages tab", async () => {
    let scanCount = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(
          scanCount === 1 ? [scannedCodexSkill] : [scannedCodexSkill, sharedLibrarySkill],
        );
      }
      if (command === "import_skillpack") {
        return Promise.resolve({ imported: 1, skillCount: 1, message: "ok" });
      }
      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    fireEvent.click(screen.getByRole("button", { name: "Packages" }));

    openMock.mockResolvedValue("C:/in/library.skillpack");
    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_skillpack", {
        packagePath: "C:/in/library.skillpack",
      }),
    );
    await waitFor(() =>
      expect(screen.getByText("Imported 1 skills from .skillpack.")).toBeInTheDocument(),
    );
  });

  it("explains why target switches are locked before a skill is imported", async () => {
    invokeMock.mockResolvedValue([scannedCodexSkill]);

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));
    openLocalScanSkill();

    const targetButton = screen.getByRole("button", { name: "Enable Claude Code" });
    expect(targetButton).toBeEnabled();
    fireEvent.click(targetButton);

    expect(
      screen.getByText("Import local-scan-skill into the shared library before changing Claude Code."),
    ).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "set_skill_target_enabled",
      expect.objectContaining({ targetId: "claude-code" }),
    );
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
    openLocalScanSkill();

    fireEvent.click(screen.getByRole("button", { name: /导入共享库/i }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_skill_to_library", {
        sourcePath: scannedCodexSkill.sourcePath,
      }),
    );
    await waitFor(() => expect(screen.getByText(/已将 local-scan-skill 导入共享库/i)).toBeInTheDocument());
    expect(screen.getAllByText("Shared Library").length).toBeGreaterThan(0);
  });

  it("imports all local skills into the shared library from one action", async () => {
    let scanCount = 0;
    invokeMock.mockImplementation((command: string, args?: { sourcePath?: string }) => {
      if (command === "scan_skills") {
        scanCount += 1;
        return Promise.resolve(
          scanCount === 1
            ? [scannedCodexSkill, secondScannedSkill]
            : [sharedLibrarySkill, secondSharedLibrarySkill],
        );
      }

      if (command === "import_skill_to_library") {
        const skillName = args?.sourcePath?.includes("second-local-skill")
          ? "second-local-skill"
          : "local-scan-skill";

        return Promise.resolve({
          imported: true,
          alreadyManaged: false,
          skillName,
          libraryPath: `C:/Users/example/.skills-manage/library/${skillName}`,
          message: "Skill imported into the shared library.",
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));

    fireEvent.click(screen.getByRole("button", { name: /全部导入/i }));

    await waitFor(() => {
      const importCalls = invokeMock.mock.calls.filter(([command]) => command === "import_skill_to_library");
      expect(importCalls).toHaveLength(2);
      expect(importCalls[0][1]).toEqual({ sourcePath: scannedCodexSkill.sourcePath });
      expect(importCalls[1][1]).toEqual({ sourcePath: secondScannedSkill.sourcePath });
    });
    await waitFor(() => expect(screen.getByText(/已导入 2 个技能到共享库/i)).toBeInTheDocument());
    expect(screen.getAllByText("Shared Library").length).toBeGreaterThan(0);
  });

  it("clears the bulk import status when the user starts a fresh scan", async () => {
    let scanCount = 0;
    let resolveManualScan: ((skills: typeof sharedLibrarySkill[]) => void) | undefined;
    invokeMock.mockImplementation((command: string, args?: { sourcePath?: string }) => {
      if (command === "scan_skills") {
        scanCount += 1;

        if (scanCount === 1) {
          return Promise.resolve([scannedCodexSkill, secondScannedSkill]);
        }

        if (scanCount === 2) {
          return Promise.resolve([sharedLibrarySkill, secondSharedLibrarySkill]);
        }

        return new Promise<typeof sharedLibrarySkill[]>((resolve) => {
          resolveManualScan = resolve;
        });
      }

      if (command === "import_skill_to_library") {
        const skillName = args?.sourcePath?.includes("second-local-skill")
          ? "second-local-skill"
          : "local-scan-skill";

        return Promise.resolve({
          imported: true,
          alreadyManaged: false,
          skillName,
          libraryPath: `C:/Users/example/.skills-manage/library/${skillName}`,
          message: "Skill imported into the shared library.",
        });
      }

      return Promise.reject(new Error(`Unexpected command: ${command}`));
    });

    render(<App />);
    await waitFor(() => expect(screen.getAllByText("local-scan-skill").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "EN" }));

    fireEvent.click(screen.getByRole("button", { name: /Import all/i }));
    await waitFor(() => expect(screen.getByText("Imported 2 skills into the shared library.")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "Scan local skills" }));

    expect(screen.getByText("Scanning local folders")).toBeInTheDocument();
    expect(screen.queryByText("Imported 2 skills into the shared library.")).not.toBeInTheDocument();

    resolveManualScan?.([sharedLibrarySkill, secondSharedLibrarySkill]);
    await waitFor(() => expect(screen.getByText("Scan complete. Showing local skills.")).toBeInTheDocument());
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
    openLocalScanSkill();

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
    openLocalScanSkill();

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
