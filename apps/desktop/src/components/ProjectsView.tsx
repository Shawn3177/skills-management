import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Plus, RefreshCw } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";
import {
  api,
  errorText,
  type Project,
  type ProjectSnapshot,
  type ProjectTarget,
} from "../lib/api";
import { allTools, filterSkills, type SkillRecord } from "../lib/skills";
import { NoticeBar, ToolLogo, type Notice } from "./ui";

export function ProjectsView({
  t,
  onOpenFolder,
}: {
  t: TFunction;
  onOpenFolder: (path: string) => void;
}) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selected, setSelected] = useState("");
  const [snapshot, setSnapshot] = useState<ProjectSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [query, setQuery] = useState("");
  const request = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    api
      .listProjects()
      .then((items) => {
        if (!mounted.current) return;
        setProjects(items);
        setSelected(items[0]?.path ?? "");
      })
      .catch((error) => setNotice({ kind: "error", text: errorText(error) }))
      .finally(() => {
        if (mounted.current) setLoading(false);
      });
    return () => {
      mounted.current = false;
      request.current += 1;
    };
  }, []);

  async function reload(path: string) {
    const id = ++request.current;
    setSnapshot(null);
    if (!path) {
      setLoading(false);
      return;
    }
    setLoading(true);
    try {
      const next = await api.scanProject(path);
      if (id === request.current && mounted.current) setSnapshot(next);
    } catch (error) {
      if (id === request.current && mounted.current)
        setNotice({ kind: "error", text: errorText(error) });
    } finally {
      if (id === request.current && mounted.current) setLoading(false);
    }
  }

  useEffect(() => {
    setNotice(null);
    void reload(selected);
    return () => {
      request.current += 1;
    };
  }, [selected]);

  useEffect(() => {
    const refresh = () => {
      if (!working) void reload(selected);
    };
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [selected, working]);

  async function add() {
    setWorking(true);
    setNotice(null);
    try {
      const path = await open({
        directory: true,
        multiple: false,
        title: t("projects.add"),
      });
      if (typeof path !== "string") return;
      const project = await api.addProject(path);
      setProjects(await api.listProjects());
      setSelected(project.path);
      if (project.path === selected) await reload(project.path);
    } catch (error) {
      setNotice({ kind: "error", text: errorText(error) });
    } finally {
      setWorking(false);
    }
  }

  async function forget() {
    setWorking(true);
    try {
      const items = await api.forgetProject(selected);
      setProjects(items);
      setSelected(items[0]?.path ?? "");
      setSnapshot(null);
    } catch (error) {
      setNotice({ kind: "error", text: errorText(error) });
    } finally {
      setWorking(false);
    }
  }

  async function deploy(
    skill: SkillRecord,
    target: ProjectTarget,
    enabled: boolean,
    confirmed = false,
    expectedFingerprint?: string,
  ) {
    if (target.state === "modified" && !expectedFingerprint) {
      setNotice({
        kind: "warning",
        text: t("projects.modifiedConfirm", {
          skill: skill.name,
          tool: toolName(target.id),
        }),
        actions: [
          {
            label: t(enabled ? "projects.sync" : "projects.disable"),
            run: () =>
              void deploy(
                skill,
                target,
                enabled,
                false,
                target.fingerprint ?? undefined,
              ),
          },
        ],
      });
      return;
    }
    if (enabled && !confirmed) {
      setWorking(true);
      try {
        const report = await api.checkCompat(skill.sourcePath, target.id);
        if (report.level !== "compatible" && !report.acknowledged) {
          setNotice({
            kind: "warning",
            text: t("projects.compat", {
              skill: skill.name,
              tool: toolName(target.id),
            }),
            actions: [
              {
                label: t("compat.enableAnyway"),
                run: () =>
                  void deploy(skill, target, true, true, expectedFingerprint),
              },
            ],
          });
          return;
        }
      } catch (error) {
        setNotice({ kind: "error", text: errorText(error) });
        return;
      } finally {
        setWorking(false);
      }
    }
    setWorking(true);
    setNotice(null);
    try {
      await api.deployProjectSkill(
        selected,
        skill.sourcePath,
        target.id,
        enabled,
        expectedFingerprint,
      );
      setNotice({
        kind: "success",
        text: t(enabled ? "projects.deployed" : "projects.disabled", {
          skill: skill.name,
          tool: toolName(target.id),
        }),
      });
      await reload(selected);
    } catch (error) {
      setNotice({ kind: "error", text: errorText(error) });
    } finally {
      setWorking(false);
    }
  }

  async function importSkill(skill: SkillRecord) {
    setWorking(true);
    setNotice(null);
    try {
      await api.importProjectSkill(selected, skill.sourcePath);
      setNotice({
        kind: "success",
        text: t("projects.imported", { skill: skill.name }),
      });
      await reload(selected);
    } catch (error) {
      setNotice({ kind: "error", text: errorText(error) });
    } finally {
      setWorking(false);
    }
  }

  function toolName(id: string) {
    return allTools.find((tool) => tool.id === id)?.name ?? id;
  }
  const matches = new Set(
    filterSkills(
      snapshot?.library.map((entry) => entry.skill) ?? [],
      query,
    ).map((skill) => skill.id),
  );
  const local = filterSkills(snapshot?.local ?? [], query);

  return (
    <div className="page-body projects-page">
      <div className="project-toolbar">
        <label className="project-select">
          <span>{t("projects.select")}</span>
          <select
            value={selected}
            disabled={working || projects.length === 0}
            onChange={(event) => setSelected(event.target.value)}
          >
            {projects.length === 0 ? (
              <option value="">{t("projects.empty")}</option>
            ) : null}
            {projects.map((project) => (
              <option key={project.path} value={project.path}>
                {project.name} — {project.path}
              </option>
            ))}
          </select>
        </label>
        <button
          className="button"
          disabled={working}
          onClick={() => void add()}
        >
          <Plus size={15} />
          {t("projects.add")}
        </button>
        <button
          className="icon-button framed"
          disabled={!selected || working || loading}
          aria-label={t("projects.refresh")}
          title={t("projects.refresh")}
          onClick={() => void reload(selected)}
        >
          <RefreshCw size={16} />
        </button>
      </div>
      {selected ? (
        <div className="project-path">
          <span className="mono">{selected}</span>
          <button
            className="icon-button"
            aria-label={t("projects.open")}
            onClick={() => onOpenFolder(selected)}
          >
            <FolderOpen size={16} />
          </button>
          <button
            className="button small"
            disabled={working}
            onClick={() =>
              setNotice({
                kind: "warning",
                text: t("projects.forgetHint"),
                actions: [
                  { label: t("projects.forget"), run: () => void forget() },
                ],
              })
            }
          >
            {t("projects.forget")}
          </button>
        </div>
      ) : null}
      <p className="field-hint">{t("projects.hint")}</p>
      <NoticeBar notice={notice} />
      {loading ? <p role="status">{t("projects.loading")}</p> : null}
      {!selected && !loading ? (
        <p className="empty-state">{t("projects.start")}</p>
      ) : null}
      {snapshot ? (
        <>
          <label className="field-group">
            <span>{t("projects.search")}</span>
            <input
              className="text-input"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={t("projects.search")}
            />
          </label>
          <section aria-labelledby="project-library">
            <h2 id="project-library">{t("projects.library")}</h2>
            {matches.size === 0 ? (
              <p className="empty-state">{t("projects.noSkills")}</p>
            ) : null}
            <div className="project-skills">
              {snapshot.library
                .filter(({ skill }) => matches.has(skill.id))
                .map(({ skill, targets }) => (
                  <article className="project-skill" key={skill.id}>
                    <div className="project-skill-heading">
                      <strong>{skill.name}</strong>
                      <p className="muted">{skill.description}</p>
                    </div>
                    <div className="project-targets">
                      {targets.map((target) => (
                        <div className="project-target" key={target.id}>
                          <span className="project-target-name">
                            <ToolLogo id={target.id} />
                            {toolName(target.id)}
                          </span>
                          <span
                            className={`tag ${target.state === "deployed" ? "success" : target.state === "available" ? "neutral" : "warning"}`}
                          >
                            {t(`projects.state.${target.state}`)}
                          </span>
                          <span
                            className="project-target-path mono"
                            title={target.path}
                          >
                            {target.path}
                          </span>
                          {target.state === "available" ||
                          target.state === "outdated" ||
                          (target.state === "modified" &&
                            target.fingerprint) ? (
                            <button
                              className="button small"
                              disabled={working}
                              aria-label={t("projects.deployLabel", {
                                skill: skill.name,
                                tool: toolName(target.id),
                              })}
                              onClick={() => void deploy(skill, target, true)}
                            >
                              {t(
                                target.state === "available"
                                  ? "projects.deploy"
                                  : "projects.sync",
                              )}
                            </button>
                          ) : null}
                          {target.state === "deployed" ||
                          target.state === "outdated" ||
                          (target.state === "modified" &&
                            target.fingerprint) ? (
                            <button
                              className="button small"
                              disabled={working}
                              aria-label={t("projects.disableLabel", {
                                skill: skill.name,
                                tool: toolName(target.id),
                              })}
                              onClick={() => void deploy(skill, target, false)}
                            >
                              {t("projects.disable")}
                            </button>
                          ) : null}
                          {target.state === "modified" ||
                          target.state === "conflict" ? (
                            <span className="field-hint">
                              {t("projects.preserve")}
                            </span>
                          ) : null}
                        </div>
                      ))}
                    </div>
                  </article>
                ))}
            </div>
          </section>
          <section aria-labelledby="project-local">
            <h2 id="project-local">{t("projects.local")}</h2>
            {local.length === 0 ? (
              <p className="field-hint">{t("projects.noLocal")}</p>
            ) : null}
            {local.map((skill) => (
              <div className="project-local" key={skill.id}>
                <div>
                  <strong>{skill.name}</strong>
                  <p className="mono muted">{skill.sourcePath}</p>
                </div>
                <button
                  className="button small"
                  disabled={working}
                  aria-label={t("projects.importLabel", { skill: skill.name })}
                  onClick={() => void importSkill(skill)}
                >
                  {t("projects.import")}
                </button>
                <button
                  className="icon-button"
                  aria-label={t("actions.openSkillFolder", {
                    skill: skill.name,
                  })}
                  onClick={() => onOpenFolder(skill.sourcePath)}
                >
                  <FolderOpen size={16} />
                </button>
              </div>
            ))}
          </section>
        </>
      ) : null}
    </div>
  );
}
