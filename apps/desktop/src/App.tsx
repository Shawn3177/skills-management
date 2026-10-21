import { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { FolderGit2, Package, Plus, Settings } from "lucide-react";
import "./App.css";
import { AddView, type AddRequest } from "./components/AddView";
import { DetailView } from "./components/DetailView";
import { EditView, type EditChanges } from "./components/EditView";
import { GithubView } from "./components/GithubView";
import { MainView, type ScanState, type Tab } from "./components/MainView";
import { SettingsView } from "./components/SettingsView";
import { SyncView } from "./components/SyncView";
import { ToolSwitcher } from "./components/ToolSwitcher";
import { NoticeBar, PageHeader, type Notice } from "./components/ui";
import { WindowControls } from "./components/WindowControls";
import { demoSkills } from "./data/demoSkills";
import { useLocale } from "./i18n/useLocale";
import { compatAsk, compatWarning } from "./lib/compat";
import {
  api,
  errorText,
  isBridgeUnavailable,
  type AppStatus,
  type GithubAccount,
  type GithubCatalog,
  type SkillUpdateStatus,
} from "./lib/api";
import {
  filterSkills,
  folderName,
  githubSource,
  partitionSkills,
  showInstalledTools,
  tools,
  type SkillRecord,
  type ToolId,
} from "./lib/skills";

type View =
  | { name: "main" }
  | { name: "edit"; path: string }
  | { name: "detail"; path: string }
  | { name: "add" }
  | { name: "sync" }
  | { name: "github" }
  | { name: "settings" };

function App() {
  const { locale, setLocale, t } = useLocale();
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [scanState, setScanState] = useState<ScanState>("scanning");
  const [view, setView] = useState<View>({ name: "main" });
  const [tab, setTab] = useState<Tab>("all");
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState<Notice>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [updates, setUpdates] = useState<SkillUpdateStatus[] | null>(null);
  const [checkedAt, setCheckedAt] = useState<Date | null>(null);
  const [checking, setChecking] = useState(false);
  const [updatingPath, setUpdatingPath] = useState<string | null>(null);
  const [appStatus, setAppStatus] = useState<AppStatus | null>(null);
  const [saving, setSaving] = useState(false);
  // Bumped when the user changes which tools are shown (tools is a live binding).
  const [, setToolsVersion] = useState(0);
  const [account, setAccount] = useState<GithubAccount | null>(null);
  const [catalog, setCatalog] = useState<GithubCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const starred = useMemo(() => new Set((catalog?.starred ?? []).map((name) => name.toLowerCase())), [catalog]);
  // Each scan gets an id so a slower, older scan can't overwrite a newer one.
  const scanRequest = useRef(0);

  // Sample records only for the browser preview (no Tauri backend).
  const records = scanState === "preview" ? demoSkills : skills;
  const { library, locals } = useMemo(() => partitionSkills(records), [records]);
  const visibleLibrary = useMemo(() => filterSkills(library, query), [library, query]);
  const visibleLocals = useMemo(() => filterSkills(locals, query), [locals, query]);
  const updateMap = useMemo(
    () => new Map((updates ?? []).map((status) => [status.libraryPath, status])),
    [updates],
  );
  const pendingUpdates = (updates ?? []).filter((status) => status.hasUpdate).length;
  // The sample records live under C:/Users/demo, so show them as ~/… too.
  const home = scanState === "preview" ? "C:/Users/demo" : appStatus?.homeDir ?? null;

  function describeError(error: unknown) {
    return isBridgeUnavailable(error) ? t("errors.bridge") : t("notice.failed", { reason: errorText(error) });
  }

  function go(next: View) {
    setNotice(null);
    setView(next);
  }

  async function loadSkills() {
    const requestId = ++scanRequest.current;
    setScanState("scanning");
    try {
      const scanned = await api.scanSkills();
      if (requestId !== scanRequest.current) {
        return;
      }
      setSkills(scanned);
      setScanState("ready");
    } catch (error) {
      if (requestId !== scanRequest.current) {
        return;
      }
      console.error("scan_skills failed", error);
      setSkills([]);
      setScanState(isBridgeUnavailable(error) ? "preview" : "error");
    }
  }

  useEffect(() => {
    void loadSkills();
    api
      .appStatus()
      .then((status) => {
        showInstalledTools(status.installedTools ?? []);
        setAppStatus(status);
      })
      .catch((error) => console.error("get_app_status failed", error));
    // The cached list needs no network; the account check asks GitHub once.
    api
      .githubCatalog(false)
      .then(setCatalog)
      .catch((error) => console.error("github_catalog failed", error));
    api
      .githubAccount()
      .then(setAccount)
      .catch((error) => console.error("github_account failed", error));

    // Pick up skills added or removed outside the app when the window comes
    // back into focus (replaces a manual rescan button).
    const onFocus = () => void loadSkills();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  function toolName(toolId: ToolId) {
    return tools.find((item) => item.id === toolId)?.name ?? toolId;
  }

  /**
   * A tool has its own, different copy of a library skill. When the skill's
   * GitHub repository is known, recommend fetching the latest version there
   * (instead of pushing an old library copy); always ask before replacing.
   */
  function askToReplace(skill: SkillRecord, toolId: ToolId) {
    const repo = githubSource(skill);
    const replace = { label: t("actions.replaceWithLibrary"), run: () => void replaceInTool(skill, toolId) };
    setNotice(
      repo
        ? {
            kind: "warning",
            text: t("notice.conflictAskGithub", {
              skill: skill.name,
              tool: toolName(toolId),
              repo: repo.replace(/^https?:\/\/(www\.)?github\.com\//, ""),
            }),
            actions: [{ label: t("actions.updateLatestAndSync"), run: () => void updateLatestAndSync(skill, toolId) }, replace],
          }
        : { kind: "warning", text: t("notice.conflictAsk", { skill: skill.name, tool: toolName(toolId) }), actions: [replace] },
    );
  }

  async function updateLatestAndSync(skill: SkillRecord, toolId: ToolId) {
    setBusyKey(`${skill.sourcePath}:${toolId}`);
    setNotice({ kind: "success", text: t("updates.updating") });
    try {
      await api.updateSkill(skill.sourcePath);
      await api.setTarget(skill.sourcePath, toolId, true, true);
      setUpdates((current) => current?.filter((status) => status.libraryPath !== skill.sourcePath) ?? null);
      setNotice({ kind: "success", text: t("notice.updatedAndSynced", { skill: skill.name, tool: toolName(toolId) }) });
    } catch (error) {
      console.error("update and sync failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  async function replaceInTool(skill: SkillRecord, toolId: ToolId) {
    setBusyKey(`${skill.sourcePath}:${toolId}`);
    setNotice(null);
    try {
      await api.setTarget(skill.sourcePath, toolId, true, true);
      setNotice({ kind: "success", text: t("notice.replaced", { skill: skill.name, tool: toolName(toolId) }) });
    } catch (error) {
      console.error("replace in tool failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  /**
   * Enabling a skill in a tool it may not suit asks first (never blocks);
   * a tool the user marked as working for this skill does not.
   */
  async function toggleTarget(skill: SkillRecord, toolId: ToolId, enable: boolean) {
    const warning = enable ? compatWarning(skill, toolId) : null;
    if (warning) {
      setNotice({
        kind: "warning",
        text: compatAsk(t, skill, toolName(toolId), warning),
        actions: [
          { label: t("compat.enableAnyway"), run: () => void applyToggle(skill, toolId, true) },
          { label: t("compat.viewDetails"), run: () => showDetail(skill) },
        ],
      });
      return;
    }
    await applyToggle(skill, toolId, enable);
  }

  function showDetail(skill: SkillRecord) {
    setNotice(null);
    go({ name: "detail", path: skill.sourcePath });
  }

  async function applyToggle(skill: SkillRecord, toolId: ToolId, enable: boolean) {
    setBusyKey(`${skill.sourcePath}:${toolId}`);
    setNotice(null);
    try {
      await api.setTarget(skill.sourcePath, toolId, enable);
      setNotice({
        kind: "success",
        text: t(enable ? "notice.enabled" : "notice.disabled", { skill: skill.name, tool: toolName(toolId) }),
      });
    } catch (error) {
      console.error("set_skill_target_enabled failed", error);
      if (enable && errorText(error).includes("different content")) {
        askToReplace(skill, toolId);
      } else {
        setNotice({
          kind: "error",
          text: errorText(error).includes("not managed")
            ? t("notice.targetConflict", { tool: toolName(toolId) })
            : describeError(error),
        });
      }
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  async function addLocalToLibrary(skill: SkillRecord) {
    setBusyKey(`add:${skill.sourcePath}`);
    setNotice(null);
    try {
      const result = await api.importSkill(skill.sourcePath);
      setNotice({ kind: "success", text: t("notice.added", { skill: result.skillName }) });
    } catch (error) {
      console.error("import_skill_to_library failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  /** Group installed skills under a GitHub repository. Resolves true on success. */
  async function linkSeries(url: string): Promise<boolean> {
    setBusyKey("series:link");
    setNotice(null);
    try {
      const result = await api.linkSeries(url);
      const values = { repo: result.repository, count: result.matched.length, total: result.skillCount };
      setNotice(
        result.matched.length > 0
          ? { kind: "success", text: t("series.linked", values) }
          : { kind: "warning", text: t("series.linkedNone", values) },
      );
      return true;
    } catch (error) {
      console.error("link_github_series failed", error);
      setNotice({ kind: "error", text: describeError(error) });
      return false;
    } finally {
      setBusyKey(null);
      await loadSkills();
    }
  }

  /** Look up unknown skill sources on GitHub. Resolves true when it ran. */
  async function findSources(): Promise<boolean> {
    setBusyKey("series:find");
    setNotice(null);
    try {
      const result = await api.findSources();
      const unknown = [...result.ambiguous, ...result.unmatched];
      const parts = [
        result.found.length > 0
          ? t("series.found", {
              count: result.found.length,
              repos: new Set(result.found.map((item) => item.repository)).size,
            })
          : t("series.foundNone"),
      ];
      if (unknown.length > 0) {
        const names = unknown.slice(0, 5).join(locale === "zh-CN" ? "、" : ", ");
        parts.push(t("series.stillUnknown", { count: unknown.length, names: unknown.length > 5 ? `${names} …` : names }));
      }
      if (result.rateLimited) {
        parts.push(t("series.rateLimited"));
      }
      if (result.failed.length > 0) {
        parts.push(t("series.failedRepos", { repos: result.failed.join(", ") }));
      }
      const clean = result.found.length > 0 && !result.rateLimited && result.failed.length === 0;
      setNotice({ kind: clean ? "success" : "warning", text: parts.join(locale === "zh-CN" ? "；" : ". ") });
      return true;
    } catch (error) {
      console.error("find_skill_sources failed", error);
      setNotice({ kind: "error", text: describeError(error) });
      return false;
    } finally {
      setBusyKey(null);
      await loadSkills();
    }
  }

  /** Read the connected account's repositories and stars again. */
  async function refreshCatalog() {
    setCatalogLoading(true);
    try {
      const fresh = await api.githubCatalog(true);
      setCatalog(fresh);
      if (fresh?.rateLimited) {
        setNotice({ kind: "warning", text: t("mine.partialNotice") });
      }
    } catch (error) {
      console.error("github_catalog refresh failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setCatalogLoading(false);
    }
  }

  /** Show the account's repositories, reading them first if never read. */
  function openGithub() {
    go({ name: "github" });
    // Lists cached by an older version only held repositories with skills.
    if (account?.login && !catalogLoading && (!catalog || catalog.avatarUrl === undefined)) {
      void refreshCatalog();
    }
  }

  async function connectGithub(token?: string): Promise<boolean> {
    setBusyKey("github:connect");
    setNotice(null);
    try {
      const connected = token === undefined ? await api.connectGithubCli() : await api.connectGithubToken(token);
      setAccount(connected);
      setNotice({ kind: "success", text: t("account.connected", { login: connected.login ?? "" }) });
      void api.appStatus().then(setAppStatus).catch(() => undefined);
      void refreshCatalog();
      return true;
    } catch (error) {
      console.error("connect github failed", error);
      setNotice({ kind: "error", text: describeError(error) });
      return false;
    } finally {
      setBusyKey(null);
    }
  }

  async function disconnectGithub() {
    setBusyKey("github:disconnect");
    setNotice(null);
    try {
      await api.disconnectGithub();
      setAccount(await api.githubAccount().catch(() => null));
      setCatalog(null);
      setNotice({ kind: "success", text: t("account.disconnected") });
      void api.appStatus().then(setAppStatus).catch(() => undefined);
    } catch (error) {
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
  }

  /** Add tool-folder skills to the shared library (one copy per folder name). */
  async function addAllToLibrary(skills: SkillRecord[]) {
    setBusyKey("locals:add-all");
    setNotice(null);
    const seen = new Set<string>();
    let added = 0;
    let lastError: unknown = null;
    for (const skill of skills) {
      const folder = folderName(skill.sourcePath).toLowerCase();
      if (seen.has(folder)) {
        continue;
      }
      seen.add(folder);
      try {
        await api.importSkill(skill.sourcePath);
        added += 1;
      } catch (error) {
        console.error("import_skill_to_library failed", error);
        lastError = error;
      }
    }
    setBusyKey(null);
    const text = t("locals.addedAll", { count: added });
    setNotice(
      lastError === null
        ? { kind: "success", text }
        : { kind: added > 0 ? "warning" : "error", text: `${text} ${describeError(lastError)}` },
    );
    await loadSkills();
  }

  async function unlinkSeries(repository: string) {
    setBusyKey(`series:unlink:${repository}`);
    setNotice(null);
    try {
      await api.unlinkSeries(repository);
      setNotice({ kind: "success", text: t("series.unlinked", { repo: repository }) });
    } catch (error) {
      console.error("unlink_github_series failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  async function runAdd(request: AddRequest) {
    setAdding(true);
    setNotice(null);
    const libraryPaths: string[] = [];
    let failed = 0;
    let lastError: unknown = null;

    try {
      if (request.kind === "local") {
        for (const path of request.paths) {
          try {
            libraryPaths.push((await api.importSkill(path)).libraryPath);
          } catch (error) {
            failed += 1;
            lastError = error;
          }
        }
      } else if (request.kind === "github") {
        libraryPaths.push((await api.importFromGithub(request.url)).libraryPath);
      } else if (request.kind === "github-many") {
        const result = await api.importGithubSkills(request.urls);
        libraryPaths.push(...result.imported.map((item) => item.libraryPath));
        if (result.failed.length > 0) {
          failed += result.failed.length;
          lastError = result.failed[0].error;
        }
      } else {
        libraryPaths.push(...(await api.importSkillpack(request.path)).libraryPaths);
      }
    } catch (error) {
      failed += 1;
      lastError = error;
    }

    // New skills are checked before they are enabled; ones that may not suit
    // a tool are held back and offered in one question at the end.
    const held: Array<{ path: string; toolId: ToolId; name: string }> = [];
    for (const path of libraryPaths) {
      for (const toolId of request.enableIn) {
        try {
          const report = await api.checkCompat(path, toolId).catch(() => null);
          if (report && report.level !== "compatible" && !report.acknowledged) {
            held.push({ path, toolId, name: folderName(path) });
            continue;
          }
          await api.setTarget(path, toolId, true);
        } catch (error) {
          console.error("enable after add failed", error);
          failed += 1;
          lastError = error;
        }
      }
    }
    setAdding(false);

    if (libraryPaths.length === 0) {
      setNotice({ kind: "error", text: describeError(lastError) });
      return;
    }
    go({ name: "main" });
    if (held.length > 0) {
      setNotice({
        kind: "warning",
        text: t("compat.skippedOnAdd", {
          added: libraryPaths.length,
          count: held.length,
          list: held.map((item) => `${item.name} → ${toolName(item.toolId)}`).join(t("compat.separator")),
        }),
        actions: [{ label: t("compat.enableSkipped"), run: () => void enableHeld(held) }],
      });
    } else {
      setNotice({
        kind: failed > 0 ? "error" : "success",
        text:
          failed > 0
            ? t("add.partial", { count: libraryPaths.length, failed })
            : t("add.done", { count: libraryPaths.length }),
      });
    }
    await loadSkills();
  }

  async function enableHeld(held: Array<{ path: string; toolId: ToolId }>) {
    setNotice(null);
    let enabled = 0;
    let lastError: unknown = null;
    for (const item of held) {
      try {
        await api.setTarget(item.path, item.toolId, true);
        enabled += 1;
      } catch (error) {
        console.error("enable held skill failed", error);
        lastError = error;
      }
    }
    setNotice(
      lastError === null
        ? { kind: "success", text: t("compat.enabledSkipped", { count: enabled }) }
        : { kind: "error", text: describeError(lastError) },
    );
    await loadSkills();
  }

  async function saveEdit(skill: SkillRecord, changes: EditChanges, confirmed = false) {
    const risky = confirmed ? [] : changes.toggles.filter((toggle) => toggle.enable && compatWarning(skill, toggle.toolId));
    if (risky.length > 0) {
      setNotice({
        kind: "warning",
        text: t("compat.askSave", {
          skill: skill.name,
          tools: risky.map((toggle) => toolName(toggle.toolId)).join(t("compat.separator")),
        }),
        actions: [
          { label: t("compat.saveAnyway"), run: () => void saveEdit(skill, changes, true) },
          { label: t("compat.viewDetails"), run: () => showDetail(skill) },
        ],
      });
      return;
    }
    setSaving(true);
    setNotice(null);
    try {
      if (changes.metadataChanged) {
        await api.updateMetadata(skill.sourcePath, changes.name, changes.description);
      }
      for (const toggle of changes.toggles) {
        await api.setTarget(skill.sourcePath, toggle.toolId, toggle.enable);
      }
      go({ name: "main" });
      setNotice({ kind: "success", text: t("edit.saved", { skill: changes.name }) });
    } catch (error) {
      console.error("saving skill failed", error);
      const conflicted = changes.toggles.find((toggle) => toggle.enable);
      if (conflicted && errorText(error).includes("different content")) {
        askToReplace(skill, conflicted.toolId);
      } else {
        setNotice({ kind: "error", text: describeError(error) });
      }
    } finally {
      setSaving(false);
    }
    await loadSkills();
  }

  async function removeFromLibrary(skill: SkillRecord) {
    setBusyKey(`remove:${skill.sourcePath}`);
    try {
      await api.removeFromLibrary(skill.sourcePath);
      go({ name: "main" });
      setNotice({ kind: "success", text: t("notice.removed", { skill: skill.name }) });
    } catch (error) {
      console.error("remove_skill_from_library failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setBusyKey(null);
    }
    await loadSkills();
  }

  async function exportSkills(list: SkillRecord[], defaultName: string) {
    if (list.length === 0) {
      setNotice({ kind: "error", text: t("export.empty") });
      return;
    }
    let destination: string | null = null;
    try {
      destination = await save({ defaultPath: defaultName, filters: [{ name: "Skillpack", extensions: ["skillpack"] }] });
    } catch (error) {
      setNotice({ kind: "error", text: describeError(error) });
      return;
    }
    if (!destination) {
      return;
    }
    try {
      const result = await api.exportSkillpack(
        list.map((skill) => ({
          sourcePath: skill.sourcePath,
          name: skill.name,
          targets: skill.targets.map((target) => ({ id: target.id, enabled: target.enabled })),
        })),
        destination,
      );
      setNotice({ kind: "success", text: t("export.done", { count: result.skillCount }) });
    } catch (error) {
      console.error("export_skillpack failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    }
  }

  async function checkUpdates() {
    setChecking(true);
    setNotice(null);
    try {
      setUpdates(await api.checkUpdates());
      setCheckedAt(new Date());
    } catch (error) {
      console.error("check_skill_updates failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setChecking(false);
    }
  }

  async function updateSkill(status: SkillUpdateStatus) {
    setUpdatingPath(status.libraryPath);
    setNotice(null);
    try {
      await api.updateSkill(status.libraryPath);
      setUpdates((current) =>
        (current ?? []).map((item) =>
          item.libraryPath === status.libraryPath
            ? { ...item, hasUpdate: false, state: "up-to-date", current: item.latest || item.current }
            : item,
        ),
      );
      setNotice({ kind: "success", text: t("updates.updated", { skill: status.skillName }) });
    } catch (error) {
      console.error("update_skill_from_github failed", error);
      setNotice({ kind: "error", text: describeError(error) });
    } finally {
      setUpdatingPath(null);
    }
    await loadSkills();
  }

  async function openFolder(path: string) {
    try {
      await revealItemInDir(path);
    } catch (error) {
      setNotice({ kind: "error", text: describeError(error) });
    }
  }

  async function openExternal(url: string) {
    try {
      await openUrl(url);
    } catch (error) {
      setNotice({ kind: "error", text: describeError(error) });
    }
  }

  const editing = view.name === "edit" ? library.find((skill) => skill.sourcePath === view.path) : undefined;
  const showing = view.name === "detail" ? records.find((skill) => skill.sourcePath === view.path) : undefined;
  const backToMain = () => go({ name: "main" });
  const pageTitle =
    view.name === "edit"
      ? t("edit.title", { skill: editing?.name ?? "" })
      : view.name === "detail"
        ? (showing?.name ?? "")
      : view.name === "add"
        ? t("add.title")
        : view.name === "sync"
          ? t("sync.title")
          : view.name === "github"
            ? t("mine.title")
            : t("settings.title");

  return (
    <main className="app">
      {view.name === "main" ? (
        <header className="top-bar" data-tauri-drag-region>
          <div className="brand" data-tauri-drag-region>
            <strong data-tauri-drag-region>{t("app.title")}</strong>
            <button
              type="button"
              className="icon-button"
              aria-label={t("nav.settings")}
              title={t("nav.settings")}
              onClick={() => go({ name: "settings" })}
            >
              <Settings size={17} strokeWidth={1.8} aria-hidden="true" />
            </button>
          </div>

          <ToolSwitcher t={t} tab={tab} library={library} onChange={setTab} />

          <button
            type="button"
            className="icon-button framed"
            aria-label={t("mine.title")}
            title={t("mine.title")}
            onClick={openGithub}
          >
            <FolderGit2 size={17} strokeWidth={1.8} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="icon-button framed"
            aria-label={pendingUpdates > 0 ? t("nav.syncWithUpdates", { count: pendingUpdates }) : t("nav.sync")}
            title={t("nav.sync")}
            onClick={() => go({ name: "sync" })}
          >
            <Package size={17} strokeWidth={1.8} aria-hidden="true" />
            {pendingUpdates > 0 ? <span className="dot" aria-hidden="true" /> : null}
          </button>
          <button type="button" className="fab" aria-label={t("nav.add")} title={t("nav.add")} onClick={() => go({ name: "add" })}>
            <Plus size={18} strokeWidth={2.6} aria-hidden="true" />
          </button>
        </header>
      ) : (
        <PageHeader title={pageTitle} backLabel={t("nav.back")} onBack={backToMain} />
      )}

      <WindowControls t={t} />
      <NoticeBar notice={notice} />

      <div className="view scroll-surface">
        {view.name === "main" ? (
          <MainView
            t={t}
            tab={tab}
            library={library}
            visibleLibrary={visibleLibrary}
            locals={visibleLocals}
            query={query}
            onQueryChange={setQuery}
            scanState={scanState}
            updates={updateMap}
            busyKey={busyKey}
            onToggle={(skill, toolId, enable) => void toggleTarget(skill, toolId, enable)}
            onOpen={(skill) => go({ name: "edit", path: skill.sourcePath })}
            onShowDetail={(skill) => go({ name: "detail", path: skill.sourcePath })}
            onOpenFolder={(path) => void openFolder(path)}
            onOpenUrl={(url) => void openExternal(url)}
            onAddToLibrary={(skill) => void addLocalToLibrary(skill)}
            onConflict={askToReplace}
            onLinkSeries={linkSeries}
            onFindSources={findSources}
            starred={starred}
            onUnlinkSeries={(repository) => void unlinkSeries(repository)}
            onAddAllToLibrary={(skills) => void addAllToLibrary(skills)}
            home={home}
          />
        ) : null}

        {view.name === "edit" ? (
          editing ? (
            <EditView
              key={editing.sourcePath}
              t={t}
              skill={editing}
              home={home}
              busy={busyKey !== null}
              saving={saving}
              onSave={(skill, changes) => void saveEdit(skill, changes)}
              onExport={(skill) => void exportSkills([skill], `${skill.name}.skillpack`)}
              onRemove={(skill) => void removeFromLibrary(skill)}
              onCancel={backToMain}
            />
          ) : (
            <div className="page-body">
              <p className="empty-state">{t("edit.missing")}</p>
            </div>
          )
        ) : null}

        {view.name === "detail" ? (
          showing ? (
            <DetailView
              key={showing.sourcePath}
              t={t}
              skill={showing}
              home={home}
              busy={busyKey !== null}
              onOpenUrl={(url) => void openExternal(url)}
              onOpenFolder={(path) => void openFolder(path)}
              onEdit={(skill) => go({ name: "edit", path: skill.sourcePath })}
              onAddToLibrary={(skill) => void addLocalToLibrary(skill)}
              onCompatChanged={() => void loadSkills()}
            />
          ) : (
            <div className="page-body">
              <p className="empty-state">{t("edit.missing")}</p>
            </div>
          )
        ) : null}

        {view.name === "add" ? (
          <AddView
            t={t}
            library={library}
            working={adding}
            onSubmit={(request) => void runAdd(request)}
            onCancel={backToMain}
            onError={(message) => setNotice({ kind: "error", text: t("notice.failed", { reason: message }) })}
            locale={locale}
            account={account}
            catalog={catalog}
            catalogLoading={catalogLoading}
            onRefreshCatalog={() => void refreshCatalog()}
            onConnectGithub={() => go({ name: "settings" })}
            onOpenUrl={(url) => void openExternal(url)}
          />
        ) : null}

        {view.name === "github" ? (
          <GithubView
            t={t}
            locale={locale}
            account={account}
            catalog={catalog}
            loading={catalogLoading || busyKey === "github:connect"}
            library={library}
            working={adding}
            onRefresh={() => void refreshCatalog()}
            onConnect={() => void connectGithub()}
            onOpenUrl={(url) => void openExternal(url)}
            onSubmit={(request) => void runAdd(request)}
          />
        ) : null}

        {view.name === "sync" ? (
          <SyncView
            t={t}
            locale={locale}
            updates={updates}
            checkedAt={checkedAt}
            checking={checking}
            updatingPath={updatingPath}
            libraryCount={library.length}
            sources={library.filter((skill) => githubSource(skill))}
            waiting={locals.filter((skill) => githubSource(skill)).length}
            onCheck={() => void checkUpdates()}
            onUpdate={(status) => void updateSkill(status)}
            onExportAll={() => void exportSkills(library, "skills-library.skillpack")}
          />
        ) : null}

        {view.name === "settings" ? (
          <SettingsView
            t={t}
            locale={locale}
            onLocaleChange={setLocale}
            appStatus={appStatus}
            onOpenFolder={(path) => void openFolder(path)}
            onNotice={setNotice}
            account={account}
            catalog={catalog}
            onOpenGithub={openGithub}
            busy={busyKey !== null}
            onConnectGithub={connectGithub}
            onDisconnectGithub={() => void disconnectGithub()}
            onToolsChanged={() => {
              setToolsVersion((version) => version + 1);
              if (tab !== "all" && !tools.some((tool) => tool.id === tab)) {
                setTab("all");
              }
            }}
            onOpenUrl={(url) => void openExternal(url)}
          />
        ) : null}
      </div>
    </main>
  );
}

export default App;
