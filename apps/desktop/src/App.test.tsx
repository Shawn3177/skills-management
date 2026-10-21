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

    expect(screen.getByRole("button", { name: "Tool: All" })).toBeInTheDocument();
    expect(screen.getByText("3 skills in the shared library · 3 enabled across 2 tools")).toBeInTheDocument();
    expect(within(card("code-review")).getByRole("button", { name: "Disable code-review in Codex" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    // Under the name: what the skill does, not its address.
    expect(within(card("code-review")).getByText("Reviews code changes.")).toBeInTheDocument();
    expect(screen.queryByText("github.com/owner/skills/tree/main/code-review")).not.toBeInTheDocument();
    // The managed copy in Claude Code's folder is not listed as a separate skill.
    expect(screen.getAllByRole("button", { name: "agent-tool-safety" })).toHaveLength(1);
    expect(screen.getByRole("img", { name: "pr-description is in Claude Code" })).toBeInTheDocument();
  });

  it("shows one switcher at the top that lists every tool, installed ones first", async () => {
    mockBackend({ get_app_status: () => ({ ...appStatus, installedTools: ["claude-code", "codex", "qwen-code"] }) });
    await renderApp();

    // Only the current choice is shown; the list opens on click.
    const trigger = screen.getByRole("button", { name: "Tool: All" });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    const options = within(screen.getByRole("listbox")).getAllByRole("option");
    expect(options).toHaveLength(13); // All + 12 tools
    await waitFor(() => expect(screen.getByRole("option", { name: "Qwen Code 0" })).toBeInTheDocument());
    expect(screen.getByRole("option", { name: "WorkBuddy Not found" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /^VS Code/ })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("option", { name: "Qwen Code 0" }));
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tool: Qwen Code" })).toBeInTheDocument();
  });

  it("switches to a tool that isn't installed and closes on Escape", async () => {
    mockBackend();
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Tool: All" }));
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Tool: All" }));
    fireEvent.click(screen.getByRole("option", { name: "Gemini CLI Not found" }));

    expect(screen.getByRole("button", { name: "Tool: Gemini CLI" })).toBeInTheDocument();
    expect(screen.getByText("3 skills in the shared library, 0 enabled in Gemini CLI")).toBeInTheDocument();
  });

  it("lets settings hide a tool from the skill cards", async () => {
    mockBackend({ get_trash_summary: () => ({ path: "x", entries: 0, totalBytes: 0, purgeableEntries: 0 }) });
    await renderApp();
    expect(within(card("code-review")).getByRole("button", { name: "Disable code-review in Codex" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    const codex = screen.getByRole("switch", { name: "Show Codex" });
    expect(codex).toBeChecked();
    fireEvent.click(codex);
    fireEvent.click(screen.getByRole("button", { name: "Back" }));

    expect(within(card("code-review")).queryByRole("button", { name: "Disable code-review in Codex" })).not.toBeInTheDocument();
    // Still reachable from the switcher, under "Other tools".
    fireEvent.click(screen.getByRole("button", { name: "Tool: All" }));
    expect(screen.getByText("Other tools")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Codex Not found" })).toBeInTheDocument();
  });

  it("uses Chinese by default", async () => {
    localStorage.clear();
    mockBackend();
    await renderApp();

    expect(screen.getByRole("button", { name: "工具：全部" })).toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "Tool: All" }));
    fireEvent.click(screen.getByRole("option", { name: /^Claude Code/ }));

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
    // Logo only; the tooltip says what differs.
    expect(chip).toHaveAttribute("title", "Codex · differs");
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

    // GitHub opens from its own button, not from the address.
    fireEvent.click(screen.getByRole("button", { name: "Open owner/skills on GitHub" }));
    expect(openUrlMock).toHaveBeenLastCalledWith("https://github.com/owner/skills");
    fireEvent.click(screen.getByRole("button", { name: "Open anthropics/skills on GitHub" }));
    expect(openUrlMock).toHaveBeenLastCalledWith("https://github.com/anthropics/skills/tree/main/skills");

    // Linked repositories are undone from the sources panel, not the group header.
    expect(screen.queryByRole("button", { name: "Unlink anthropics/skills" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Find sources" }));
    fireEvent.click(screen.getByRole("button", { name: "Unlink anthropics/skills" }));
    await waitFor(() => expect(calls("unlink_github_series")).toEqual([{ repository: "anthropics/skills" }]));
  });

  it("adds every tool-folder skill to the shared library with one confirmed button", async () => {
    const codexSkill = (name: string, source = "Codex") => ({
      ...localOnly,
      id: `${source}-${name}`,
      name,
      source,
      sourcePath: `C:/Users/example/.${source === "Codex" ? "codex" : "claude"}/skills/${name}`,
      detectedSourceUrl: `https://github.com/anthropics/skills/tree/main/skills/${name}`,
      linkedSource: true,
    });
    mockBackend({ import_skill_to_library: (args) => ({ imported: true, skillName: "x", libraryPath: `${LIB}/x`, args }) }, [
      review,
      codexSkill("pdf"),
      codexSkill("xlsx"),
      // The same skill in another tool's folder is added once.
      codexSkill("pdf", "Claude Code"),
    ]);
    await renderApp();

    // The series header shows where its skills live instead of a button.
    expect(screen.getByRole("img", { name: "anthropics/skills is in Claude Code, Codex" })).toBeInTheDocument();

    // Each group can also be added on its own.
    expect(screen.getByRole("button", { name: "Add the anthropics/skills group to the shared library" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Add all to the shared library" }));
    expect(calls("import_skill_to_library")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Add 2" }));

    await waitFor(() =>
      expect(calls("import_skill_to_library")).toEqual([
        { sourcePath: "C:/Users/example/.codex/skills/pdf" },
        { sourcePath: "C:/Users/example/.codex/skills/xlsx" },
      ]),
    );
    await waitFor(() =>
      expect(screen.getByText("Added 2 skills to the shared library.")).toBeInTheDocument(),
    );
  });

  it("finds where installed skills came from and reports the ones it could not place", async () => {
    mockBackend({
      find_skill_sources: () => ({
        found: [
          { folder: "pr-description", repository: "owner/skills", url: "https://github.com/owner/skills/tree/main/pr-description", exact: true },
        ],
        unmatched: ["skill-evals"],
        ambiguous: [],
        repositories: ["anthropics/skills", "owner/skills"],
        failed: [],
        rateLimited: false,
      }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "1 without a source · find them" }));

    await waitFor(() => expect(calls("find_skill_sources")).toHaveLength(1));
    await waitFor(() =>
      expect(
        screen.getByText("Found sources for 1 skills in 1 repositories. 1 still unknown: skill-evals"),
      ).toBeInTheDocument(),
    );
  });

  it("offers automatic source finding first in the sources panel", async () => {
    mockBackend({
      find_skill_sources: () => ({ found: [], unmatched: [], ambiguous: [], repositories: [], failed: [], rateLimited: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Find sources" }));
    fireEvent.click(screen.getByRole("button", { name: "Find automatically" }));

    await waitFor(() =>
      expect(
        screen.getByText(
          "No new sources found. GitHub's hourly limit was reached before every repository was checked; try again later",
        ),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByRole("region", { name: "Find where skills came from" })).not.toBeInTheDocument();
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

    fireEvent.click(screen.getByRole("button", { name: "Find sources" }));
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

  it("shows a skill's details and SKILL.md when its name is clicked, not GitHub", async () => {
    mockBackend({
      read_skill_markdown: () => "---\nname: code-review\ndescription: Reviews code changes.\n---\n# How to review\n\n- Read the diff\n<script>alert(1)</script>\n",
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "code-review" }));

    expect(openUrlMock).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { level: 1, name: "code-review" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "How to review" })).toBeInTheDocument();
    expect(screen.getByText("Read the diff")).toBeInTheDocument();
    expect(document.querySelector(".markdown-body script")).toBeNull();
    expect(calls("read_skill_markdown")).toEqual([{ skillPath: review.sourcePath }]);

    fireEvent.click(screen.getByRole("button", { name: "Open code-review on GitHub" }));
    expect(openUrlMock).toHaveBeenCalledWith("https://github.com/owner/skills/tree/main/code-review");
  });

  const todoFinding = {
    ruleId: "cc-tools",
    level: "degraded",
    token: "TodoWrite",
    file: "SKILL.md",
    line: 7,
    excerpt: "Track steps with TodoWrite.",
  };
  const codexWarning = { targetId: "codex", level: "degraded", count: 2, first: todoFinding, acknowledged: false };

  it("asks before enabling a skill in a tool it may not suit, and enables only when confirmed", async () => {
    mockBackend(
      { set_skill_target_enabled: () => ({ enabled: true, changed: true }) },
      [review, { ...safety, compat: [codexWarning] }, broken],
    );
    await renderApp();

    const button = within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" });
    expect(button).toHaveAttribute("title", "Codex · uses the Claude Code tool TodoWrite (SKILL.md line 7)");
    fireEvent.click(button);

    expect(
      await screen.findByText(
        "Parts of agent-tool-safety may not work in Codex: it uses the Claude Code tool TodoWrite (SKILL.md line 7), plus 1 more.",
      ),
    ).toBeInTheDocument();
    expect(calls("set_skill_target_enabled")).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "Enable anyway" }));
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: safety.sourcePath, targetId: "codex", enabled: true },
      ]),
    );
  });

  it("says when a tool got its own version of a skill, and lists the versions on the details page", async () => {
    const dev = {
      ...safety,
      variants: [{ targetId: "codex", path: ".agents/skills/dev" }],
    };
    mockBackend(
      {
        set_skill_target_enabled: () => ({ enabled: true, changed: true, variant: ".agents/skills/dev" }),
        read_skill_markdown: () => "Body",
      },
      [review, dev, broken],
    );
    await renderApp();

    fireEvent.click(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" }));
    expect(
      await screen.findByText(
        "Enabled agent-tool-safety in Codex, using the repository's own Codex version (.agents/skills/dev).",
      ),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    expect(await screen.findByText("Tool versions")).toBeInTheDocument();
    expect(screen.getByText("Codex: .agents/skills/dev")).toBeInTheDocument();
  });

  it("switches a tool between the repository's version and the main version on the details page", async () => {
    const dev = { ...safety, variants: [{ targetId: "codex", path: ".agents/skills/dev", choice: "variant" }] };
    let scans = 0;
    mockBackend({
      scan_skills: () => {
        scans += 1;
        return scans === 1
          ? [review, dev, broken]
          : [review, { ...dev, variants: [{ ...dev.variants[0], choice: "default" }] }, broken];
      },
      read_skill_markdown: () => "Body",
      set_skill_variant_choice: () => ({ targetId: "codex", choice: "default", refreshed: true, message: "" }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    const group = await screen.findByRole("group", { name: "Version for Codex" });
    const own = within(group).getByRole("button", { name: "Repository's Codex version" });
    const main = within(group).getByRole("button", { name: "Main version" });
    expect(own).toHaveAttribute("aria-pressed", "true");
    expect(main).toHaveAttribute("aria-pressed", "false");

    // The version already in use does nothing.
    fireEvent.click(own);
    expect(calls("set_skill_variant_choice")).toHaveLength(0);

    fireEvent.click(main);
    await waitFor(() =>
      expect(calls("set_skill_variant_choice")).toEqual([
        { libraryPath: safety.sourcePath, targetId: "codex", choice: "default" },
      ]),
    );
    expect(await screen.findByText("Codex now uses the main version. Synced to Codex.")).toBeInTheDocument();
    await waitFor(() =>
      expect(within(screen.getByRole("group", { name: "Version for Codex" })).getByRole("button", { name: "Main version" })).toHaveAttribute(
        "aria-pressed",
        "true",
      ),
    );
  });

  it("detects tool versions for a skill added from GitHub before they were kept", async () => {
    const fromGithub = { ...safety, originKind: "github", sourceUrl: "https://github.com/o/r/tree/main/skills/dev" };
    let scans = 0;
    mockBackend({
      scan_skills: () => {
        scans += 1;
        return scans === 1
          ? [review, fromGithub, broken]
          : [review, { ...fromGithub, variants: [{ targetId: "codex", path: ".codex/skills/dev", choice: "variant" }] }, broken];
      },
      read_skill_markdown: () => "Body",
      detect_skill_variants: () => ({
        skillName: "agent-tool-safety",
        variants: [{ targetId: "codex", path: ".codex/skills/dev", choice: "variant" }],
        refreshedTargets: ["Codex"],
        failedTargets: [],
        message: "",
      }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    fireEvent.click(await screen.findByRole("button", { name: "Detect tool versions" }));
    await waitFor(() => expect(calls("detect_skill_variants")).toEqual([{ libraryPath: safety.sourcePath }]));
    expect(await screen.findByText("Found tool versions for: Codex. Updated the copy in: Codex.")).toBeInTheDocument();
    expect(await screen.findByText("Codex: .codex/skills/dev")).toBeInTheDocument();
  });

  it("offers no version controls for a library skill without versions or a GitHub source", async () => {
    mockBackend({ read_skill_markdown: () => "Body" }, [review, safety, broken]);
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    expect(await screen.findByText("Location")).toBeInTheDocument();
    expect(screen.queryByText("Tool versions")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Detect tool versions" })).not.toBeInTheDocument();
  });

  it("does not ask again in a tool the user marked as working", async () => {
    mockBackend(
      { set_skill_target_enabled: () => ({ enabled: true, changed: true }) },
      [review, { ...safety, compat: [{ ...codexWarning, acknowledged: true }] }, broken],
    );
    await renderApp();

    fireEvent.click(within(card("agent-tool-safety")).getByRole("button", { name: "Enable agent-tool-safety in Codex" }));

    await waitFor(() => expect(calls("set_skill_target_enabled")).toHaveLength(1));
    expect(screen.queryByRole("button", { name: "Enable anyway" })).not.toBeInTheDocument();
  });

  it("lists what the check found on the details page and marks a tool as working", async () => {
    const risky = { ...safety, compat: [codexWarning] };
    let scans = 0;
    mockBackend({
      scan_skills: () => {
        scans += 1;
        return scans === 1 ? [review, risky, broken] : [review, { ...risky, compat: [{ ...codexWarning, acknowledged: true }] }, broken];
      },
      read_skill_markdown: () => "---\nname: agent-tool-safety\ndescription: x\n---\nBody\n",
      check_skill_compat: () => ({
        targetId: "codex",
        targetName: "Codex",
        level: "degraded",
        declared: null,
        findings: [todoFinding, { ...todoFinding, ruleId: "cc-args", token: "$ARGUMENTS", line: 9, excerpt: "Pass $ARGUMENTS on." }],
        acknowledged: false,
      }),
      acknowledge_skill_compat: () => ({ acknowledged: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "agent-tool-safety" }));
    const panel = await screen.findByRole("region", { name: "Tool fit" });
    expect(within(panel).getByText("Partly limited")).toBeInTheDocument();
    expect(within(panel).getByText("uses the Claude Code tool TodoWrite")).toBeInTheDocument();

    fireEvent.click(within(panel).getByRole("button", { name: "Show all 2" }));
    expect(await within(panel).findByText("uses Claude Code's $ARGUMENTS placeholder")).toBeInTheDocument();
    expect(calls("check_skill_compat")).toEqual([{ sourcePath: safety.sourcePath, targetId: "codex" }]);

    fireEvent.click(within(panel).getByRole("button", { name: "Mark as working" }));
    await waitFor(() => expect(calls("acknowledge_skill_compat")).toEqual([{ sourcePath: safety.sourcePath, targetId: "codex" }]));
    expect(await within(panel).findByText("Confirmed working")).toBeInTheDocument();
  });

  it("asks before saving an edit that enables a tool the skill may not suit", async () => {
    mockBackend(
      { set_skill_target_enabled: () => ({ enabled: true, changed: true }) },
      [review, { ...safety, compat: [codexWarning] }, broken],
    );
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Edit agent-tool-safety" }));
    fireEvent.click(screen.getByRole("button", { name: "Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(
      await screen.findByText("agent-tool-safety may not suit Codex; the details page shows why. Save anyway?"),
    ).toBeInTheDocument();
    expect(calls("set_skill_target_enabled")).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "Save anyway" }));
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([
        { sourcePath: safety.sourcePath, targetId: "codex", enabled: true },
      ]),
    );
  });

  it("opens skill details and removes a skill only after confirming", async () => {
    mockBackend({ remove_skill_from_library: () => ({ skillName: "agent-tool-safety", removedFrom: ["Claude Code"] }) });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Edit agent-tool-safety" }));

    expect(screen.getByRole("heading", { level: 1, name: "Edit skill · agent-tool-safety" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "✓ Claude Code" })).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: "Remove from library" }));
    expect(calls("remove_skill_from_library")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Confirm (move to trash)" }));

    await waitFor(() => expect(calls("remove_skill_from_library")).toEqual([{ libraryPath: safety.sourcePath }]));
    await waitFor(() => expect(screen.getByText("Moved agent-tool-safety to trash.")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Tool: All" })).toBeInTheDocument();
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
    fireEvent.click(screen.getByRole("button", { name: "Edit agent-tool-safety" }));
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

    fireEvent.click(screen.getByRole("button", { name: "Edit code-review" }));

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
    expect(screen.getByRole("button", { name: "Tool: All" })).toBeInTheDocument();
  });

  it("connects GitHub through the CLI sign-in, reads the account's repositories, and disconnects", async () => {
    let connected = false;
    mockBackend({
      github_account: () => ({ connected, login: connected ? "octo" : null, name: null, source: connected ? "stored" : null }),
      github_catalog: (args) =>
        args?.refresh ? { login: "octo", fetchedAt: 0, repos: [], starred: [], scanned: 3, rateLimited: false } : null,
      connect_github_cli: () => {
        connected = true;
        return { connected: true, login: "octo", name: null, source: "stored" };
      },
      disconnect_github: () => {
        connected = false;
      },
      get_trash_summary: () => ({ path: "x", entries: 0, totalBytes: 0, purgeableEntries: 0 }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await waitFor(() => expect(screen.getByText("Not connected")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Use GitHub CLI sign-in" }));

    await waitFor(() => expect(screen.getByText("Connected as @octo")).toBeInTheDocument());
    await waitFor(() => expect(calls("github_catalog")).toContainEqual({ refresh: true }));

    fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
    await waitFor(() => expect(calls("disconnect_github")).toHaveLength(1));
    await waitFor(() => expect(screen.getByText("Not connected")).toBeInTheDocument());
  });

  it("adds skills picked from the account's own and starred repositories", async () => {
    const skill = (repo: string, folder: string) => ({
      folder,
      path: `skills/${folder}`,
      url: `https://github.com/${repo}/tree/main/skills/${folder}`,
      blob: folder,
    });
    mockBackend({
      github_account: () => ({ connected: true, login: "octo", name: null, source: "stored" }),
      github_catalog: () => ({
        login: "octo",
        fetchedAt: 0,
        starred: ["owner/skills"],
        scanned: 2,
        rateLimited: false,
        repos: [
          {
            fullName: "octo/kit",
            description: "My skills",
            htmlUrl: "https://github.com/octo/kit",
            defaultBranch: "main",
            owned: true,
            starred: false,
            fork: false,
            stars: 1,
            skills: [skill("octo/kit", "pdf"), skill("octo/kit", "code-review")],
          },
          {
            fullName: "owner/skills",
            description: null,
            htmlUrl: "https://github.com/owner/skills",
            defaultBranch: "main",
            owned: false,
            starred: true,
            fork: false,
            stars: 900,
            skills: [skill("owner/skills", "tdd")],
          },
        ],
      }),
      import_github_skills: (args) => ({
        imported: (args?.urls as string[]).map((url) => ({ skillName: url, libraryPath: `${LIB}/${url.split("/").pop()}`, url })),
        failed: [],
      }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add skill" }));
    fireEvent.click(screen.getByRole("button", { name: "My GitHub" }));
    await waitFor(() => expect(screen.getByText("octo/kit")).toBeInTheDocument());

    // "Starred" narrows the list; "All" brings it back.
    fireEvent.click(screen.getByRole("button", { name: "Starred" }));
    expect(screen.queryByText("octo/kit")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "All" }));

    // code-review is already in the library, so selecting the repository picks only pdf.
    fireEvent.click(screen.getByRole("checkbox", { name: "Select every skill in octo/kit" }));
    fireEvent.click(screen.getByRole("button", { name: "Add 1 skills" }));

    await waitFor(() =>
      expect(calls("import_github_skills")).toEqual([{ urls: ["https://github.com/octo/kit/tree/main/skills/pdf"] }]),
    );
    await waitFor(() =>
      expect(calls("set_skill_target_enabled")).toEqual([{ sourcePath: `${LIB}/pdf`, targetId: "claude-code", enabled: true }]),
    );
  });

  it("shows every own and starred repository on the My GitHub page", async () => {
    const repo = (fullName: string, extra: Record<string, unknown>) => ({
      fullName,
      description: null,
      htmlUrl: `https://github.com/${fullName}`,
      defaultBranch: "main",
      owned: false,
      starred: false,
      fork: false,
      stars: 0,
      skills: [],
      private: false,
      language: null,
      pushedAt: "2026-10-01T00:00:00Z",
      ...extra,
    });
    mockBackend({
      github_account: () => ({ connected: true, login: "octo", name: "Octo Cat", source: "stored" }),
      github_catalog: () => ({
        login: "octo",
        name: "Octo Cat",
        avatarUrl: null,
        fetchedAt: 0,
        starred: ["owner/skills"],
        scanned: 2,
        rateLimited: false,
        repos: [
          repo("octo/notes", { owned: true, description: "Plain notes", language: "TypeScript", private: true }),
          repo("owner/skills", {
            starred: true,
            stars: 1200,
            skills: [{ folder: "tdd", path: "tdd", url: "https://github.com/owner/skills/tree/main/tdd", blob: "x" }],
          }),
        ],
      }),
      import_github_skills: () => ({ imported: [{ skillName: "tdd", libraryPath: `${LIB}/tdd`, url: "" }], failed: [] }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "My GitHub" }));

    expect(await screen.findByText("Octo Cat")).toBeInTheDocument();
    expect(screen.getByText("octo/notes")).toBeInTheDocument();
    expect(screen.getByText("Plain notes")).toBeInTheDocument();
    expect(screen.getByText("No skills")).toBeInTheDocument();
    expect(screen.getByText("1 own repositories · 1 starred · 1 with skills · updated", { exact: false })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "With skills" }));
    expect(screen.queryByText("octo/notes")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "Select every skill in owner/skills" }));
    fireEvent.click(screen.getByRole("button", { name: "Add 1 skills" }));
    await waitFor(() =>
      expect(calls("import_github_skills")).toEqual([{ urls: ["https://github.com/owner/skills/tree/main/tdd"] }]),
    );
  });

  it("marks a series whose repository the account starred", async () => {
    const docx = { ...review, id: "docx", name: "docx", sourcePath: `${LIB}/docx`, sourceUrl: "https://github.com/owner/skills/tree/main/docx" };
    mockBackend(
      { github_catalog: () => ({ login: "octo", fetchedAt: 0, repos: [], starred: ["Owner/Skills"], scanned: 0, rateLimited: false }) },
      [review, docx, safety],
    );
    render(<App />);

    await waitFor(() => expect(screen.getByRole("img", { name: "Starred" })).toBeInTheDocument());
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

  it("holds back enabling a new skill in a tool it may not suit until asked", async () => {
    mockBackend({
      import_from_github: () => ({ skillName: "remote", libraryPath: `${LIB}/remote` }),
      check_skill_compat: (args) => ({
        targetId: args?.targetId,
        level: args?.targetId === "codex" ? "incompatible" : "compatible",
        declared: null,
        findings: [],
        acknowledged: false,
      }),
      set_skill_target_enabled: () => ({ enabled: true }),
    });
    await renderApp();

    fireEvent.click(screen.getByRole("button", { name: "Add skill" }));
    fireEvent.click(screen.getByRole("button", { name: "GitHub" }));
    fireEvent.change(screen.getByRole("textbox", { name: /GitHub URL/ }), {
      target: { value: "https://github.com/owner/repo/tree/main/remote" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Add" }));

    expect(
      await screen.findByText("Added 1 skills. 1 may not suit their tool and were not enabled: remote → Codex."),
    ).toBeInTheDocument();
    expect(calls("set_skill_target_enabled")).toEqual([
      { sourcePath: `${LIB}/remote`, targetId: "claude-code", enabled: true },
    ]);

    fireEvent.click(screen.getByRole("button", { name: "Enable them anyway" }));
    await waitFor(() => expect(screen.getByText("Enabled 1.")).toBeInTheDocument());
    expect(calls("set_skill_target_enabled")).toHaveLength(2);
    expect(calls("set_skill_target_enabled")[1]).toEqual({ sourcePath: `${LIB}/remote`, targetId: "codex", enabled: true });
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
    expect(screen.getByText("Not connected")).toBeInTheDocument();
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
