import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

const invokeMock = vi.fn();
const saveMock = vi.fn();
const openMock = vi.fn();
const openUrlMock = vi.fn();
const revealMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: (...args: unknown[]) => saveMock(...args),
  open: (...args: unknown[]) => openMock(...args),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
  revealItemInDir: (...args: unknown[]) => revealMock(...args),
}));

const LIB = "C:/Users/example/.skills-manage/library";

const targets = (claude: boolean, codex: boolean) => [
  { id: "claude-code", name: "Claude Code", enabled: claude },
  { id: "codex", name: "Codex", enabled: codex },
  { id: "vs-code", name: "VS Code", enabled: false },
];

const review = {
  id: "code-review",
  name: "code-review",
  description: "Reviews code changes.",
  source: "Shared Library",
  sourcePath: `${LIB}/code-review`,
  sourceUrl: "https://github.com/owner/skills/tree/main/code-review",
  health: "healthy",
  targets: targets(true, true),
  supportFiles: ["SKILL.md", "references/checklist.md"],
};

const safety = {
  id: "agent-tool-safety",
  name: "agent-tool-safety",
  description: "Designs safe tools.",
  source: "Shared Library",
  sourcePath: `${LIB}/agent-tool-safety`,
  health: "warning",
  targets: targets(true, false),
  supportFiles: ["SKILL.md"],
};

const broken = {
  id: "stale-pack",
  name: "stale-pack",
  description: "SKILL.md could not be read.",
  source: "Shared Library",
  sourcePath: `${LIB}/stale-pack`,
  health: "broken",
  targets: targets(false, false),
  supportFiles: ["SKILL.md"],
};

const managedCopy = {
  ...safety,
  id: "agent-tool-safety-copy",
  source: "Claude Code",
  sourcePath: "C:/Users/example/.claude/skills/agent-tool-safety",
  managed: true,
};

const localOnly = {
  id: "pr-description",
  name: "pr-description",
  description: "Writes PR descriptions.",
  source: "Claude Code",
  sourcePath: "C:/Users/example/.claude/skills/pr-description",
  health: "healthy",
  targets: targets(true, false),
  supportFiles: ["SKILL.md"],
};

const appStatus = {
  appName: "Skills Manage",
  version: "0.1.0",
  dataRoot: "D:\\data\\.skills-manage",
  safeWriteMode: "copy-with-trash",
  githubTokenConfigured: false,
};

type Handler = (args?: Record<string, unknown>) => unknown;

function mockBackend(handlers: Record<string, Handler> = {}, scan: unknown[] = [review, safety, broken, managedCopy, localOnly]) {
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    const handler = handlers[command];
    if (handler) {
      return Promise.resolve().then(() => handler(args));
    }
    if (command === "scan_skills") {
      return Promise.resolve(scan);
    }
    if (command === "get_app_status") {
      return Promise.resolve(appStatus);
    }
    return Promise.reject(new Error(`Unexpected command: ${command}`));
  });
}

function calls(command: string) {
  return invokeMock.mock.calls.filter(([name]) => name === command).map(([, args]) => args);
}

async function renderApp() {
  render(<App />);
  await waitFor(() => expect(screen.getByRole("button", { name: "code-review" })).toBeInTheDocument());
}

function card(name: string) {
  return screen.getByRole("button", { name }).closest("article") as HTMLElement;
}

describe("App", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    saveMock.mockReset();
    openMock.mockReset();
    openUrlMock.mockReset();
    revealMock.mockReset();
    localStorage.clear();
    localStorage.setItem("skills-manage.locale", "en-US");
  });

  afterEach(() => {
    cleanup();
  });

  it("opens on the All tab with every library skill and its tools", async () => {
    mockBackend();
    await renderApp();

    expect(screen.getByRole("tab", { name: /^All/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("3 skills in the shared library · 3 enabled across 2 tools")).toBeInTheDocument();
    expect(within(card("code-review")).getByRole("button", { name: "Disable code-review in Codex" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByText("github.com/owner/skills/tree/main/code-review")).toBeInTheDocument();
    // The managed copy in Claude Code's folder is not listed as a separate skill.
    expect(screen.getAllByRole("button", { name: "agent-tool-safety" })).toHaveLength(1);
    expect(screen.getByText("Only in Claude Code")).toBeInTheDocument();
  });

  it("uses Chinese by default", async () => {
    localStorage.clear();
    mockBackend();
    await renderApp();

    expect(screen.getByRole("tab", { name: /^全部/ })).toBeInTheDocument();
    expect(screen.getByText("共享库 3 个技能 · 在 2 个工具中共启用 3 处")).toBeInTheDocument();
  });

  it("shows sample records with a calm note in a plain browser", async () => {
    invokeMock.mockRejectedValue(new Error("Cannot read properties of undefined (reading 'invoke')"));
    render(<App />);

    await waitFor(() =>
      expect(screen.getByText("Desktop scan runs in the app window. Showing sample records.")).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "agent-tool-safety" })).toBeInTheDocument();
    expect(screen.queryByText(/reading 'invoke'/)).not.toBeInTheDocument();
  });

  it("shows an empty state when the library is empty", async () => {
    mockBackend({}, [localOnly]);
    render(<App />);

    await waitFor(() => expect(screen.getByText("Your shared library is empty")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Add pr-description to the shared library" })).toBeInTheDocument();
  });

  it("enables a skill for a tool from its chip and rescans", async () => {
    let scans = 0;
    mockBackend({
      scan_skills: () => {
        scans += 1;
        return scans === 1
          ? [review, safety, broken]
          : [review, { ...safety, targets: targets(true, true) }, broken];
      },
      set_skill_target_enabled: () => ({ enabled: true, changed: true }),
    });
    await renderApp();

    fireEvent.click(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" }));

    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: safety.sourcePath, targetId: "codex", enabled: true },
      ]),
    );
    await waitFor(() => expect(screen.getByText("Enabled agent-tool-safety in Codex.")).toBeInTheDocument());
    await waitFor(() =>
      expect(within(card("agent-tool-safety")).getByRole("button", { name: "Disable agent-tool-safety in Codex" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  it("filters to one tool and toggles with an enable or disable button", async () => {
    mockBackend({ set_skill_target_enabled: () => ({ enabled: false, changed: true }) });
    await renderApp();

    fireEvent.click(screen.getByRole("tab", { name: /^Claude Code/ }));

    expect(screen.getByText("3 skills in the shared library, 2 enabled in Claude Code")).toBeInTheDocument();
    expect(within(card("code-review")).getByText("Enabled")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Disable code-review in Claude Code" }));

    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: review.sourcePath, targetId: "claude-code", enabled: false },
      ]),
    );
  });

  it("explains a target folder conflict instead of showing the raw error", async () => {
    mockBackend({
      set_skill_target_enabled: () => {
        throw "Target folder already exists and is not managed by Skills Manage: C:/x";
      },
    });
    await renderApp();

    fireEvent.click(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Codex already has a folder with that name that Skills Manage doesn't manage.",
      ),
    );
    expect(screen.queryByText(/C:\/x/)).not.toBeInTheDocument();
  });

  it("asks before replacing a tool's own, different copy of a library skill", async () => {
    const conflicted = {
      ...safety,
      targets: [
        { id: "claude-code", name: "Claude Code", enabled: true },
        { id: "codex", name: "Codex", enabled: false, conflict: true },
      ],
    };
    mockBackend({ set_skill_target_enabled: () => ({ enabled: true, changed: true }) }, [review, conflicted]);
    await renderApp();

    const chip = within(card("agent-tool-safety")).getByRole("button", {
      name: "agent-tool-safety in Codex differs from the library version",
    });
    expect(chip).toHaveTextContent("Codex · differs");
    fireEvent.click(chip);

    expect(screen.getByRole("alert")).toHaveTextContent("Codex already has a agent-tool-safety folder whose content differs");
    expect(calls("set_skill_target_enabled")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Replace with library version" }));

    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: safety.sourcePath, targetId: "codex", enabled: true, replaceExisting: true },
      ]),
    );
    await waitFor(() =>
      expect(screen.getByText("Replaced agent-tool-safety in Codex with the library version. The old folder moved to trash.")).toBeInTheDocument(),
    );
  });

  it("recommends updating to the latest GitHub version when the source is known", async () => {
    const conflicted = {
      ...safety,
      detectedSourceUrl: "https://github.com/Panniantong/Agent-Reach",
      targets: [
        { id: "claude-code", name: "Claude Code", enabled: true },
        { id: "codex", name: "Codex", enabled: false, conflict: true },
      ],
    };
    mockBackend(
      {
        update_skill_from_github: () => ({ skillName: "agent-tool-safety" }),
        set_skill_target_enabled: () => ({ enabled: true }),
      },
      [review, conflicted],
    );
    await renderApp();

    fireEvent.click(
      within(card("agent-tool-safety")).getByRole("button", { name: "agent-tool-safety in Codex differs from the library version" }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("It comes from Panniantong/Agent-Reach on GitHub");
    fireEvent.click(screen.getByRole("button", { name: "Update to latest from GitHub" }));

    await waitFor(() => expect(calls("update_skill_from_github")).toEqual([{ libraryPath: safety.sourcePath }]));
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: safety.sourcePath, targetId: "codex", enabled: true, replaceExisting: true },
      ]),
    );
    await waitFor(() =>
      expect(screen.getByText("Updated agent-tool-safety to the latest version from GitHub and synced it to Codex.")).toBeInTheDocument(),
    );
  });

  it("offers the replace choice when enabling hits a different same-named folder", async () => {
    mockBackend({
      set_skill_target_enabled: (args) => {
        if (!args?.replaceExisting) {
          throw "Target folder already exists with different content and is not managed by Skills Manage: C:/x";
        }
        return { enabled: true };
      },
    });
    await renderApp();

    fireEvent.click(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" }));

    await waitFor(() => expect(screen.getByRole("button", { name: "Replace with library version" })).toBeInTheDocument());
    expect(screen.queryByText(/C:\/x/)).not.toBeInTheDocument();
  });

  it("does not let a broken skill be enabled", async () => {
    mockBackend();
    await renderApp();

    for (const chip of within(card("stale-pack")).getAllByRole("button", { name: "stale-pack needs a fix first" })) {
      expect(chip).toBeDisabled();
    }
    expect(within(card("stale-pack")).getByText("SKILL.md can't be read. Fix it before enabling.")).toBeInTheDocument();
  });

  it("adds a tool's own skill to the shared library", async () => {
    mockBackend({
      import_skill_to_library: () => ({ imported: true, skillName: "pr-description", libraryPath: `${LIB}/pr-description` }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add pr-description to the shared library" }));

    await waitFor(() => expect(calls("import_skill_to_library")).toEqual([{ sourcePath: localOnly.sourcePath }]));
    await waitFor(() => expect(screen.getByText("Added pr-description to the shared library.")).toBeInTheDocument());
  });

  it("folds skills from one GitHub repository into a series that opens on GitHub", async () => {
    const docx = { ...review, id: "docx", name: "docx", sourcePath: `${LIB}/docx`, sourceUrl: "https://github.com/owner/skills/tree/main/docx" };
    const codexSkill = (name: string) => ({
      ...localOnly,
      id: name,
      name,
      source: "Codex",
      sourcePath: `C:/Users/example/.codex/skills/${name}`,
      detectedSourceUrl: `https://github.com/anthropics/skills/tree/main/skills/${name}`,
      linkedSource: true,
    });
    mockBackend({ unlink_github_series: () => 2 }, [review, docx, safety, codexSkill("pdf"), codexSkill("xlsx")]);
    render(<App />);

    const toggle = await screen.findByRole("button", { name: "Expand owner/skills" });
    expect(screen.getByRole("button", { name: "agent-tool-safety" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "code-review" })).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(screen.getByRole("button", { name: "Collapse owner/skills" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "code-review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Unlink owner/skills" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Open owner/skills on GitHub" }));
    expect(openUrlMock).toHaveBeenLastCalledWith("https://github.com/owner/skills");
    fireEvent.click(screen.getByRole("button", { name: "Open anthropics/skills on GitHub" }));
    expect(openUrlMock).toHaveBeenLastCalledWith("https://github.com/anthropics/skills/tree/main/skills");

    fireEvent.click(screen.getByRole("button", { name: "Unlink anthropics/skills" }));
    await waitFor(() => expect(calls("unlink_github_series")).toEqual([{ repository: "anthropics/skills" }]));
  });

  it("links a GitHub repository so installed skills are grouped under it", async () => {
    mockBackend({
      link_github_series: () => ({
        repository: "anthropics/skills",
        url: "https://github.com/anthropics/skills/tree/main/skills",
        skillCount: 17,
        matched: ["docx", "pdf"],
        truncated: false,
      }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Link repo" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Link a GitHub repository" }), {
      target: { value: "https://github.com/anthropics/skills" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Link" }));

    await waitFor(() => expect(calls("link_github_series")).toEqual([{ url: "https://github.com/anthropics/skills" }]));
    await waitFor(() =>
      expect(screen.getByText("Linked anthropics/skills: 2 installed skills grouped (17 in the repository)")).toBeInTheDocument(),
    );
    expect(screen.queryByRole("textbox", { name: "Link a GitHub repository" })).not.toBeInTheDocument();
  });

  it("filters library skills with the search box", async () => {
    mockBackend();
    await renderApp();

    fireEvent.change(screen.getByRole("searchbox", { name: "Search skills" }), { target: { value: "owner/skills" } });

    expect(screen.getByRole("button", { name: "code-review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "agent-tool-safety" })).not.toBeInTheDocument();
  });

  it("opens skill details and removes a skill only after confirming", async () => {
    mockBackend({ remove_skill_from_library: () => ({ skillName: "agent-tool-safety", removedFrom: ["Claude Code"] }) });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));

    expect(screen.getByRole("heading", { level: 1, name: "Edit skill · agent-tool-safety" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "✓ Claude Code" })).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: "Remove from library" }));
    expect(calls("remove_skill_from_library")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Confirm (move to trash)" }));

    await waitFor(() => expect(calls("remove_skill_from_library")).toEqual([{ libraryPath: safety.sourcePath }]));
    await waitFor(() => expect(screen.getByText("Moved agent-tool-safety to trash.")).toBeInTheDocument());
    expect(screen.getByRole("tab", { name: /^All/ })).toBeInTheDocument();
  });

  it("saves a new name, description and tool selection from the edit page", async () => {
    const missingName = { ...safety, issues: ["missing-name"] };
    mockBackend(
      {
        update_skill_metadata: () => ({ name: "Tool safety", refreshedTargets: [], failedTargets: [] }),
        set_skill_target_enabled: () => ({ enabled: true }),
      },
      [review, missingName],
    );
    await renderApp();

    expect(within(card("agent-tool-safety")).getByText("No name")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    expect(screen.getByText("SKILL.md has no name field. Saving writes the values above into it.")).toBeInTheDocument();

    fireEvent.change(screen.getByRole("textbox", { name: "Skill name" }), { target: { value: "Tool safety" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Description" }), { target: { value: "Safer tools." } });
    fireEvent.click(screen.getByRole("button", { name: "Codex" }));
    expect(calls("set_skill_target_enabled")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(calls("update_skill_metadata")).toEqual([
        { libraryPath: safety.sourcePath, name: "Tool safety", description: "Safer tools." },
      ]),
    );
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([{ sourcePath: safety.sourcePath, targetId: "codex", enabled: true }]),
    );
    await waitFor(() => expect(screen.getByText("Saved Tool safety.")).toBeInTheDocument());
  });

  it("opens a skill's folder from the card and exports it from the edit page", async () => {
    mockBackend({ export_skillpack: () => ({ skillCount: 1 }) });
    saveMock.mockResolvedValue("C:/out/code-review.skillpack");
    revealMock.mockResolvedValue(undefined);
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Open the folder of code-review" }));
    expect(revealMock).toHaveBeenCalledWith(review.sourcePath);

    fireEvent.click(screen.getByRole("button", { name: "code-review" }));

    fireEvent.click(screen.getByRole("button", { name: "Export .skillpack" }));
    await waitFor(() =>
      expect(calls("export_skillpack")[0]).toEqual(
        expect.objectContaining({
          destination: "C:/out/code-review.skillpack",
          sources: [expect.objectContaining({ sourcePath: review.sourcePath, name: "code-review" })],
        }),
      ),
    );
    await waitFor(() => expect(screen.getByText("Exported 1 skills.")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("tab", { name: /^All/ })).toBeInTheDocument();
  });

  it("adds a skill from GitHub and enables it in the chosen tools", async () => {
    mockBackend({
      import_from_github: () => ({ skillName: "remote", libraryPath: `${LIB}/remote` }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add skill" }));
    fireEvent.click(screen.getByRole("button", { name: "GitHub" }));
    fireEvent.change(screen.getByRole("textbox", { name: /GitHub URL/ }), {
      target: { value: "https://github.com/owner/repo/tree/main/remote" },
    });
    // Claude Code is preselected; add Codex too.
    expect(screen.getByRole("button", { name: "✓ Claude Code" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(calls("import_from_github")).toEqual([{ url: "https://github.com/owner/repo/tree/main/remote" }]),
    );
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: `${LIB}/remote`, targetId: "claude-code", enabled: true },
        { sourcePath: `${LIB}/remote`, targetId: "codex", enabled: true },
      ]),
    );
    await waitFor(() => expect(screen.getByText("Added 1 skills.")).toBeInTheDocument());
  });

  it("adds skills found in a local folder, skipping ones already in the library", async () => {
    const candidate = { ...localOnly, sourcePath: "D:/skills/pr-description" };
    const alreadyThere = { ...review, source: "Manual Path", sourcePath: "D:/skills/code-review" };
    openMock.mockResolvedValue("D:/skills");
    mockBackend({
      scan_skill_folder: () => [candidate, alreadyThere],
      import_skill_to_library: () => ({ imported: true, skillName: "pr-description", libraryPath: `${LIB}/pr-description` }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add skill" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose…" }));

    await waitFor(() => expect(screen.getByText(/Found 1 skills here that aren't in the library/)).toBeInTheDocument());
    expect(screen.getByText("(1 already in the library were skipped)")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add 1 skills" }));

    await waitFor(() => expect(calls("import_skill_to_library")).toEqual([{ sourcePath: "D:/skills/pr-description" }]));
    await waitFor(() => expect(screen.getByText("Added 1 skills.")).toBeInTheDocument());
  });

  it("imports a .skillpack from the add page", async () => {
    openMock.mockResolvedValue("C:/in/team.skillpack");
    mockBackend({
      import_skillpack: () => ({ imported: 2, skillCount: 2, libraryPaths: [`${LIB}/a`, `${LIB}/b`] }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add skill" }));
    fireEvent.click(screen.getByRole("button", { name: ".skillpack" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose a .skillpack file…" }));
    await waitFor(() => expect(screen.getByText("C:/in/team.skillpack")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Import" }));

    await waitFor(() => expect(calls("import_skillpack")).toEqual([{ packagePath: "C:/in/team.skillpack" }]));
    await waitFor(() => expect(screen.getByText("Added 2 skills.")).toBeInTheDocument());
  });

  it("checks for GitHub updates, flags them on the main screen, and updates a skill", async () => {
    mockBackend({
      check_skill_updates: () => [
        {
          libraryPath: review.sourcePath,
          skillName: "code-review",
          state: "update-available",
          hasUpdate: true,
          current: "a1b2c3d",
          latest: "e4f5a6b",
          url: "https://github.com/owner/skills/tree/main/code-review",
          message: "",
        },
      ],
      update_skill_from_github: () => ({ skillName: "code-review" }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Packages and sync" }));
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    await waitFor(() => expect(screen.getByText(/a1b2c3d → e4f5a6b/)).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(within(card("code-review")).getByText("Update")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Packages and sync (1 updates)" }));

    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await waitFor(() => expect(calls("update_skill_from_github")).toEqual([{ libraryPath: review.sourcePath }]));
    await waitFor(() => expect(screen.getByText("Updated code-review.")).toBeInTheDocument());
    expect(screen.getByText("Up to date")).toBeInTheDocument();
  });

  it("shows settings and clears old trash only after confirming", async () => {
    let purged = false;
    mockBackend({
      get_trash_summary: () => ({ path: "x", entries: purged ? 1 : 2, totalBytes: 2048, purgeableEntries: purged ? 0 : 1 }),
      purge_trash: () => {
        purged = true;
        return { removed: 1, failed: 0 };
      },
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));

    expect(screen.getByText("D:\\data\\.skills-manage")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("Skills Manage 0.1.0")).toBeInTheDocument());
    expect(screen.getByText("Not set")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("2 items · 2.0 KB")).toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "Clear items older than 30 days" }));
    expect(calls("purge_trash")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Permanently delete 1 items" }));

    await waitFor(() => expect(calls("purge_trash")).toEqual([{ olderThanDays: 30 }]));
    await waitFor(() => expect(screen.getByText("Permanently deleted 1 items.")).toBeInTheDocument());
  });

  it("switches the interface language from settings", async () => {
    mockBackend({ get_trash_summary: () => ({ path: "x", entries: 0, totalBytes: 0, purgeableEntries: 0 }) });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "中文" }));

    expect(screen.getByRole("heading", { level: 1, name: "设置" })).toBeInTheDocument();
    expect(localStorage.getItem("skills-manage.locale")).toBe("zh-CN");
  });
});
