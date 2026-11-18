import "@testing-library/jest-dom/vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PublicSkillPicker } from "./PublicSkillPicker";
import { api, type PublicPreview, type PublicSearchResults } from "../lib/api";
import { formatMessage } from "../i18n/messages";

vi.mock("../lib/api", () => ({
  errorText: (error: Error) => error.message,
  api: {
    searchPublicSkills: vi.fn(),
    resolvePublicSkill: vi.fn(),
    previewPublicSkill: vi.fn(),
  },
}));
const t = (
  key: Parameters<typeof formatMessage>[1],
  params?: Parameters<typeof formatMessage>[2],
) => formatMessage("en-US", key, params);
const result: PublicSearchResults = {
  skills: [
    {
      id: "author/repo/pdf",
      name: "pdf",
      source: "author/repo",
      skillId: "pdf",
      installs: 12,
    },
  ],
  unsupported: 0,
};
const preview: PublicPreview = {
  previewId: "pinned",
  name: "pdf",
  source: "author/repo",
  path: "skills/pdf",
  description: "PDF tools",
  commit: "a".repeat(40),
  url: "https://github.com/author/repo",
  markdown:
    "# Preview\n![Tracking pixel](https://example.com/pixel)\n<script>alert(1)</script>",
  files: ["SKILL.md"],
  installedPath: null,
};
const onSelected = vi.fn();
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.searchPublicSkills).mockResolvedValue(result);
  vi.mocked(api.resolvePublicSkill).mockResolvedValue({
    repositoryId: "repo",
    paths: ["skills/pdf"],
    suggestedPath: "skills/pdf",
  });
  vi.mocked(api.previewPublicSkill).mockResolvedValue(preview);
});
afterEach(cleanup);

it("does not submit a query while confirming Chinese input", () => {
  mount();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "代码审查" } });
  fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter", isComposing: true });
  expect(api.searchPublicSkills).not.toHaveBeenCalled();
});

function mount() {
  render(
    <PublicSkillPicker
      t={t}
      working={false}
      onSelected={onSelected}
      onOpenUrl={vi.fn()}
    />,
  );
}
function search(query = "pdf") {
  fireEvent.change(screen.getByRole("textbox"), { target: { value: query } });
  fireEvent.click(screen.getByRole("button", { name: "Search" }));
}

it("discards responses from earlier queries", async () => {
  let finish!: (result: PublicSearchResults) => void;
  vi.mocked(api.searchPublicSkills)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValueOnce({ skills: [], unsupported: 0 });
  mount();
  search("first");
  search("second");
  await screen.findByText(/No matching skills/);
  await act(async () => finish(result));
  expect(
    screen.queryByRole("button", { name: /pdf.*author/ }),
  ).not.toBeInTheDocument();
});

it("requires an explicit folder choice when the indexed name is ambiguous", async () => {
  vi.mocked(api.resolvePublicSkill).mockResolvedValue({
    repositoryId: "repo",
    paths: ["skills/pdf", ".claude/skills/pdf"],
    suggestedPath: null,
  });
  mount();
  search();
  fireEvent.click(await screen.findByRole("button", { name: /pdf.*author/ }));
  const folders = await screen.findByRole("combobox", {
    name: "Skill folder in repository",
  });
  expect(api.previewPublicSkill).not.toHaveBeenCalled();
  expect(onSelected).toHaveBeenLastCalledWith(null);
  fireEvent.change(folders, { target: { value: "skills/pdf" } });
  await waitFor(() => expect(onSelected).toHaveBeenLastCalledWith(preview));
  expect(api.previewPublicSkill).toHaveBeenCalledWith("repo", "skills/pdf");
  expect(document.querySelector(".public-preview img")).toBeNull();
  expect(document.querySelector(".public-preview script")).toBeNull();
});

it("shows network errors separately from empty search results", async () => {
  vi.mocked(api.searchPublicSkills).mockRejectedValue(
    new Error("Search is offline"),
  );
  mount();
  search();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Search is offline",
  );
  expect(screen.queryByText(/No matching skills/)).not.toBeInTheDocument();
  expect(onSelected).toHaveBeenLastCalledWith(null);
});

it("does not select an old preview after another result is selected", async () => {
  let finish!: (value: PublicPreview) => void;
  vi.mocked(api.previewPublicSkill)
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    )
    .mockResolvedValueOnce({ ...preview, previewId: "new" });
  mount();
  search();
  const row = await screen.findByRole("button", { name: /pdf.*author/ });
  fireEvent.click(row);
  await waitFor(() => expect(finish).toBeDefined());
  fireEvent.click(row);
  await waitFor(() =>
    expect(onSelected).toHaveBeenLastCalledWith({
      ...preview,
      previewId: "new",
    }),
  );
  await act(async () => finish(preview));
  expect(onSelected).toHaveBeenLastCalledWith({ ...preview, previewId: "new" });
});
