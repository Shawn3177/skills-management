import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ProjectsView } from "./ProjectsView";
import { api, type ProjectSnapshot } from "../lib/api";
import { formatMessage } from "../i18n/messages";

vi.mock("../lib/api", () => ({
  errorText: (error: Error) => error.message,
  api: { listProjects: vi.fn(), addProject: vi.fn(), forgetProject: vi.fn(), scanProject: vi.fn(),
    deployProjectSkill: vi.fn(), importProjectSkill: vi.fn(), checkCompat: vi.fn() },
}));
const openFolder = vi.fn();
const { pickFolder } = vi.hoisted(() => ({ pickFolder: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: pickFolder }));
const projects = [{ name: "项目 A", path: "D:/项目 A" }, { name: "Project B", path: "D:/Project B" }];
const snapshot: ProjectSnapshot = {
  library: [{ skill: { id: "review", name: "review", description: "Review code", source: "Shared Library", sourcePath: "C:/library/review", health: "healthy", targets: [], supportFiles: [] },
    targets: [{ id: "codex", path: "D:/项目 A/.agents/skills/review", state: "available" }] }],
  local: [],
};
const t = (key: Parameters<typeof formatMessage>[1], params?: Parameters<typeof formatMessage>[2]) => formatMessage("zh-CN", key, params);

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.listProjects).mockResolvedValue(projects);
  vi.mocked(api.scanProject).mockResolvedValue(snapshot);
  vi.mocked(api.checkCompat).mockResolvedValue({ targetId: "codex", targetName: "Codex", level: "compatible", declared: null, findings: [], acknowledged: false });
  vi.mocked(api.deployProjectSkill).mockResolvedValue(undefined);
});
afterEach(cleanup);

it("selects the canonical project when adding an alias of an existing folder", async () => {
  pickFolder.mockResolvedValue("D:/alias-to-A");
  vi.mocked(api.addProject).mockResolvedValue(projects[0]);
  render(<ProjectsView t={t} onOpenFolder={openFolder} />);
  await screen.findByRole("button", { name: "将 review 部署到项目的 Codex" });
  fireEvent.click(screen.getByRole("button", { name: "添加项目" }));
  await waitFor(() => expect(api.addProject).toHaveBeenCalledWith("D:/alias-to-A"));
  await waitFor(() => expect(screen.getByRole("combobox")).toHaveValue(projects[0].path));
  expect(screen.getByRole("button", { name: "添加项目" })).toBeEnabled();
});

it("deploys only to the selected project and shows the destination", async () => {
  render(<ProjectsView t={t} onOpenFolder={openFolder} />);
  await screen.findByRole("button", { name: "将 review 部署到项目的 Codex" });
  expect(screen.getByText("D:/项目 A/.agents/skills/review")).toBeInTheDocument();
  fireEvent.change(screen.getByRole("combobox"), { target: { value: projects[1].path } });
  await waitFor(() => expect(api.scanProject).toHaveBeenLastCalledWith(projects[1].path));
  fireEvent.click(await screen.findByRole("button", { name: "将 review 部署到项目的 Codex" }));
  await waitFor(() => expect(api.deployProjectSkill).toHaveBeenCalledWith(projects[1].path, "C:/library/review", "codex", true, undefined));
  expect(await screen.findByText("已将 review 部署到项目的 Codex。")).toBeInTheDocument();
});

it("requires confirmation tied to the displayed revision before replacing local edits", async () => {
  vi.mocked(api.scanProject).mockResolvedValue({ ...snapshot, library: [{ ...snapshot.library[0], targets: [{ ...snapshot.library[0].targets[0], state: "modified", fingerprint: "revision-1" }] }] });
  render(<ProjectsView t={t} onOpenFolder={openFolder} />);
  fireEvent.click(await screen.findByRole("button", { name: "将 review 部署到项目的 Codex" }));
  expect(api.deployProjectSkill).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "同步共享库" }));
  await waitFor(() => expect(api.deployProjectSkill).toHaveBeenCalledWith(projects[0].path, "C:/library/review", "codex", true, "revision-1"));
});

it("prevents an earlier scan from replacing a newer project selection", async () => {
  let finish: ((result: ProjectSnapshot) => void) | undefined;
  vi.mocked(api.scanProject).mockImplementation((path) => path === projects[0].path ? new Promise((resolve) => { finish = resolve; }) : Promise.resolve({ library: [], local: [] }));
  render(<ProjectsView t={t} onOpenFolder={openFolder} />);
  await waitFor(() => expect(finish).toBeDefined());
  fireEvent.change(screen.getByRole("combobox"), { target: { value: projects[1].path } });
  await screen.findByText("没有匹配的共享库技能。可先从下方项目目录导入，或返回技能库添加。");
  finish!(snapshot);
  await waitFor(() => expect(screen.queryByText("review")).not.toBeInTheDocument());
});

it("imports a project skill through the scoped backend command", async () => {
  const local = { ...snapshot.library[0].skill, sourcePath: "D:/项目 A/.claude/skills/review", source: "Claude Code" };
  vi.mocked(api.scanProject).mockResolvedValue({ library: [], local: [local] });
  render(<ProjectsView t={t} onOpenFolder={openFolder} />);
  fireEvent.click(await screen.findByRole("button", { name: "将 review 保存到共享库" }));
  await waitFor(() => expect(api.importProjectSkill).toHaveBeenCalledWith(projects[0].path, local.sourcePath));
});
