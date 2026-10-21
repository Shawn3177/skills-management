import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Package, Plus } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";
import { api, errorText, type GithubAccount, type GithubCatalog } from "../lib/api";
import { folderName, tools, type SkillRecord, type ToolId } from "../lib/skills";
import { MyGithubPicker } from "./MyGithubPicker";
import { Chip } from "./ui";

export type AddRequest =
  | { kind: "local"; paths: string[]; enableIn: ToolId[] }
  | { kind: "github"; url: string; enableIn: ToolId[] }
  | { kind: "github-many"; urls: string[]; enableIn: ToolId[] }
  | { kind: "pack"; path: string; enableIn: ToolId[] };

type Source = "local" | "github" | "mine" | "pack";

type Props = {
  t: TFunction;
  library: SkillRecord[];
  working: boolean;
  onSubmit: (request: AddRequest) => void;
  onCancel: () => void;
  onError: (message: string) => void;
  locale: string;
  account: GithubAccount | null;
  catalog: GithubCatalog | null;
  catalogLoading: boolean;
  onRefreshCatalog: () => void;
  onConnectGithub: () => void;
  onOpenUrl: (url: string) => void;
};

export function AddView(props: Props) {
  const { t, library, working, onSubmit, onCancel, onError } = props;
  const [source, setSource] = useState<Source>("local");
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [folder, setFolder] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [candidates, setCandidates] = useState<SkillRecord[]>([]);
  const [skipped, setSkipped] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [githubUrl, setGithubUrl] = useState("");
  const [packPath, setPackPath] = useState<string | null>(null);
  // Claude Code is preselected, as in the design; the user can clear it.
  const [enableIn, setEnableIn] = useState<Set<ToolId>>(new Set<ToolId>(["claude-code"]));

  async function chooseFolder() {
    let picked: string | string[] | null = null;
    try {
      picked = await open({ directory: true, multiple: false });
    } catch (error) {
      onError(errorText(error));
      return;
    }
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) {
      return;
    }

    setFolder(path);
    setScanning(true);
    try {
      const found = await api.scanFolder(path);
      const libraryFolders = new Set(library.map((skill) => folderName(skill.sourcePath).toLowerCase()));
      const fresh = found.filter(
        (skill) =>
          skill.source !== "Shared Library" &&
          !skill.managed &&
          !libraryFolders.has(folderName(skill.sourcePath).toLowerCase()),
      );
      setCandidates(fresh);
      setSkipped(found.length - fresh.length);
      setSelected(new Set(fresh.filter((skill) => skill.health !== "broken").map((skill) => skill.sourcePath)));
    } catch (error) {
      onError(errorText(error));
    } finally {
      setScanning(false);
    }
  }

  async function choosePack() {
    try {
      const picked = await open({ multiple: false, filters: [{ name: "Skillpack", extensions: ["skillpack"] }] });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (path) {
        setPackPath(path);
      }
    } catch (error) {
      onError(errorText(error));
    }
  }

  function toggleSet<T>(set: Set<T>, value: T): Set<T> {
    const next = new Set(set);
    if (next.has(value)) {
      next.delete(value);
    } else {
      next.add(value);
    }
    return next;
  }

  const enabledTools = [...enableIn];
  const canSubmit =
    !working &&
    (source === "local"
      ? selected.size > 0
      : source === "github"
        ? githubUrl.trim().length > 0
        : source === "mine"
          ? picked.size > 0
          : Boolean(packPath));
  const submitLabel = working
    ? t("add.working")
    : source === "local"
      ? t("add.submitCount", { count: selected.size })
      : source === "mine"
        ? t("add.submitCount", { count: picked.size })
      : source === "pack"
        ? t("add.submitPack")
        : t("add.submit");

  function submit() {
    if (!canSubmit) {
      return;
    }
    if (source === "local") {
      onSubmit({ kind: "local", paths: [...selected], enableIn: enabledTools });
    } else if (source === "github") {
      onSubmit({ kind: "github", url: githubUrl.trim(), enableIn: enabledTools });
    } else if (source === "mine") {
      onSubmit({ kind: "github-many", urls: [...picked], enableIn: enabledTools });
    } else if (packPath) {
      onSubmit({ kind: "pack", path: packPath, enableIn: enabledTools });
    }
  }

  const sources: Array<{ id: Source; label: string; hint: string }> = [
    { id: "local", label: t("add.local"), hint: t("add.localHint") },
    { id: "github", label: t("add.github"), hint: t("add.githubHint") },
    { id: "mine", label: t("add.mine"), hint: t("add.mineHint") },
    { id: "pack", label: t("add.pack"), hint: t("add.packHint") },
  ];

  return (
    <>
      <div className="page-body">
        <form
          className="panel"
          onSubmit={(event) => {
            event.preventDefault();
            submit();
          }}
        >
          <div className="field-group">
            <span className="field-label" id="add-source-label">
              {t("add.from")}
            </span>
            <div className="chip-row" role="group" aria-labelledby="add-source-label">
              {sources.map((item) => (
                <Chip
                key={item.id}
                pressed={source === item.id}
                onClick={() => {
                  setSource(item.id);
                  // The first visit reads the account's repositories once.
                  if (item.id === "mine" && !props.catalog && !props.catalogLoading && props.account?.login) {
                    props.onRefreshCatalog();
                  }
                }}
              >
                  {item.label}
                </Chip>
              ))}
            </div>
            <span className="field-hint">{sources.find((item) => item.id === source)?.hint}</span>
          </div>

          {source === "local" ? (
            <div className="field-group">
              <span className="field-label">{t("add.folder")}</span>
              <span className="picker-row">
                <span className={`picker-value mono ${folder ? "" : "empty"}`}>{folder ?? t("add.noFolder")}</span>
                <button type="button" className="button" onClick={() => void chooseFolder()}>
                  {t("add.chooseFolder")}
                </button>
              </span>
              {scanning ? <span className="field-hint">{t("add.scanning")}</span> : null}
              {folder && !scanning ? (
                candidates.length > 0 ? (
                  <>
                    <span className="field-label found-label">
                      {t("add.found", { count: candidates.length })}
                      {skipped > 0 ? <span className="muted"> {t("add.skipped", { count: skipped })}</span> : null}
                    </span>
                    <div className="candidate-list">
                      {candidates.map((skill) => (
                        <label className="candidate" key={skill.sourcePath}>
                          <input
                            type="checkbox"
                            checked={selected.has(skill.sourcePath)}
                            disabled={skill.health === "broken"}
                            onChange={() => setSelected((current) => toggleSet(current, skill.sourcePath))}
                          />
                          <span>
                            <strong>{skill.name}</strong>
                            <small>{skill.health === "broken" ? t("card.broken") : skill.description}</small>
                          </span>
                        </label>
                      ))}
                    </div>
                  </>
                ) : (
                  <span className="field-hint">{t("add.noneFound")}</span>
                )
              ) : null}
            </div>
          ) : null}

          {source === "github" ? (
            <label className="field-group">
              <span className="field-label">{t("add.githubUrl")}</span>
              <input
                className="text-input"
                type="url"
                placeholder="https://github.com/owner/repo/tree/main/skills/foo"
                value={githubUrl}
                onChange={(event) => setGithubUrl(event.currentTarget.value)}
              />
              <span className="field-hint">{t("add.githubUrlHint")}</span>
            </label>
          ) : null}

          {source === "mine" ? (
            <MyGithubPicker
              t={t}
              locale={props.locale}
              mode="add"
              account={props.account}
              catalog={props.catalog}
              loading={props.catalogLoading}
              libraryFolders={new Set(library.map((skill) => folderName(skill.sourcePath).toLowerCase()))}
              selected={picked}
              onSelectedChange={setPicked}
              onRefresh={props.onRefreshCatalog}
              onConnect={props.onConnectGithub}
              onOpenUrl={props.onOpenUrl}
            />
          ) : null}

          {source === "pack" ? (
            <div className="drop-zone">
              <Package size={26} strokeWidth={1.6} aria-hidden="true" />
              <span className="mono">{packPath ?? t("add.packNone")}</span>
              <button type="button" className="button" onClick={() => void choosePack()}>
                {t("add.packChoose")}
              </button>
            </div>
          ) : null}

          <div className="field-group separated">
            <span className="field-label" id="add-enable-label">
              {t("add.enableAfter")}
            </span>
            <div className="chip-row" role="group" aria-labelledby="add-enable-label">
              {tools.map((tool) => (
                <Chip
                  key={tool.id}
                  pressed={enableIn.has(tool.id)}
                  onClick={() => setEnableIn((current) => toggleSet(current, tool.id))}
                >
                  {enableIn.has(tool.id) ? `✓ ${tool.name}` : tool.name}
                </Chip>
              ))}
            </div>
          </div>
        </form>
      </div>

      <footer className="page-footer">
        <span className="spacer" />
        <button type="button" className="button ghost" onClick={onCancel}>
          {t("actions.cancel")}
        </button>
        <button type="button" className="button primary" disabled={!canSubmit} onClick={submit}>
          <Plus size={15} strokeWidth={2.4} aria-hidden="true" />
          {submitLabel}
        </button>
      </footer>
    </>
  );
}
