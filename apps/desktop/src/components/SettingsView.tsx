import { useEffect, useState } from "react";
import { locales, type Locale } from "../i18n/messages";
import type { TFunction } from "../i18n/useLocale";
import {
  api,
  errorText,
  formatBytes,
  type AppStatus,
  type GithubAccount,
  type GithubCatalog,
  type TrashSummary,
} from "../lib/api";
import { allTools, isToolInstalled, isToolShown, setToolShown } from "../lib/skills";
import { ToolLogo, type Notice } from "./ui";

/** Full language names for the settings switch (the language's own name). */
const languageNames: Record<Locale, string> = { "zh-CN": "中文", "en-US": "English" };

/** Trash entries older than this many days can be cleared. */
export const TRASH_PURGE_DAYS = 30;

type Props = {
  t: TFunction;
  locale: Locale;
  onLocaleChange: (locale: Locale) => void;
  appStatus: AppStatus | null;
  onOpenFolder: (path: string) => void;
  onNotice: (notice: Notice) => void;
  account: GithubAccount | null;
  catalog: GithubCatalog | null;
  onOpenGithub: () => void;
  busy: boolean;
  /** Without a token: use GitHub CLI's sign-in. Resolves true when connected. */
  onConnectGithub: (token?: string) => Promise<boolean>;
  onDisconnectGithub: () => void;
  onOpenUrl: (url: string) => void;
  /** Which tools are shown changed (tabs and toggles need a refresh). */
  onToolsChanged: () => void;
};

/** A classic token needs no scopes to read public repositories and stars. */
const NEW_TOKEN_URL = "https://github.com/settings/tokens/new?description=Skills%20Manage";

export function SettingsView({
  t,
  locale,
  onLocaleChange,
  appStatus,
  onOpenFolder,
  onNotice,
  account,
  catalog,
  onOpenGithub,
  busy,
  onConnectGithub,
  onDisconnectGithub,
  onOpenUrl,
  onToolsChanged,
}: Props) {
  const [trash, setTrash] = useState<TrashSummary | null>(null);
  const [pasting, setPasting] = useState(false);
  const [token, setToken] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [purging, setPurging] = useState(false);

  async function loadTrash() {
    try {
      setTrash(await api.trashSummary(TRASH_PURGE_DAYS));
    } catch (error) {
      console.error("get_trash_summary failed", error);
      setTrash(null);
    }
  }

  useEffect(() => {
    void loadTrash();
  }, []);

  async function purge() {
    setConfirming(false);
    setPurging(true);
    try {
      const result = await api.purgeTrash(TRASH_PURGE_DAYS);
      onNotice({
        kind: result.failed > 0 ? "error" : "success",
        text:
          result.failed > 0
            ? t("settings.purgedPartial", { count: result.removed, failed: result.failed })
            : t("settings.purged", { count: result.removed }),
      });
    } catch (error) {
      onNotice({ kind: "error", text: t("notice.failed", { reason: errorText(error) }) });
    } finally {
      setPurging(false);
    }
    await loadTrash();
  }

  const dataRoot = appStatus?.dataRoot ?? "%USERPROFILE%\\.skills-manage";

  return (
    <div className="page-body narrow">
      <section className="panel" aria-labelledby="settings-general">
        <h2 id="settings-general">{t("settings.general")}</h2>
        <div className="setting-row">
          <span className="setting-label">{t("settings.language")}</span>
          <div className="segmented" role="group" aria-label={t("language.label")}>
            {locales.map((item) => (
              <button
                type="button"
                key={item}
                aria-pressed={locale === item}
                className={locale === item ? "active" : ""}
                onClick={() => onLocaleChange(item)}
              >
                {languageNames[item]}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <span className="setting-label">
            {t("settings.dataRoot")}
            <small className="mono">{dataRoot}</small>
          </span>
          <button type="button" className="button small" onClick={() => onOpenFolder(dataRoot)}>
            {t("settings.openFolder")}
          </button>
        </div>
      </section>

      <section className="panel" aria-labelledby="settings-tools">
        <div className="panel-head">
          <h2 id="settings-tools">{t("settings.tools")}</h2>
          <span className="muted small">{t("settings.toolsHint")}</span>
        </div>
        <ul className="tool-list">
          {allTools.map((tool) => {
            const installed = isToolInstalled(tool.id);
            const shown = isToolShown(tool.id);
            return (
              <li key={tool.id} className="tool-row">
                <ToolLogo id={tool.id} dim={!installed} />
                <span className="setting-label">
                  {tool.name}
                  <small className="mono">{tool.folder}</small>
                </span>
                <span className={`tag ${installed ? "success" : "neutral"}`}>
                  {installed ? t("settings.installed") : t("settings.notInstalled")}
                </span>
                <label className="switch">
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label={t("settings.showTool", { tool: tool.name })}
                    checked={shown}
                    onChange={(event) => {
                      setToolShown(tool.id, event.currentTarget.checked);
                      onToolsChanged();
                    }}
                  />
                  <span aria-hidden="true" />
                </label>
              </li>
            );
          })}
        </ul>
      </section>

      <section className="panel" aria-labelledby="settings-github">
        <h2 id="settings-github">{t("account.title")}</h2>
        <div className="setting-row">
          <span className="setting-label">
            {account?.connected && account.login
              ? t("account.signedIn", { login: account.login })
              : account?.connected
                ? t("account.broken")
                : t("account.notConnected")}
            <small>
              {account?.connected && account.login && catalog && catalog.avatarUrl !== undefined
                ? t("account.summary", {
                    owned: catalog.repos.filter((repo) => repo.owned).length,
                    starred: catalog.starred.length,
                    skills: catalog.repos.filter((repo) => repo.skills.length > 0).length,
                  })
                : account?.source === "environment"
                  ? t("account.fromEnvironment")
                  : t("account.hint")}
            </small>
          </span>
          {account?.connected && account.login ? (
            <span className="confirm-group">
              <button type="button" className="button small primary" onClick={onOpenGithub}>
                {t("account.viewRepos")}
              </button>
              {account.source === "stored" ? (
                <button type="button" className="button small" disabled={busy} onClick={onDisconnectGithub}>
                  {t("account.disconnect")}
                </button>
              ) : null}
            </span>
          ) : account?.connected && account.source === "stored" ? (
            <button type="button" className="button small" disabled={busy} onClick={onDisconnectGithub}>
              {t("account.disconnect")}
            </button>
          ) : account?.source !== "environment" ? (
            <span className="confirm-group">
              <button type="button" className="button small" disabled={busy} onClick={() => setPasting((value) => !value)}>
                {t("account.pasteToken")}
              </button>
              <button type="button" className="button small primary" disabled={busy} onClick={() => void onConnectGithub()}>
                {t("account.useCli")}
              </button>
            </span>
          ) : null}
        </div>
        {pasting && !(account?.connected && account.login) ? (
          <form
            className="token-row"
            onSubmit={(event) => {
              event.preventDefault();
              void onConnectGithub(token).then((ok) => {
                if (ok) {
                  setToken("");
                  setPasting(false);
                }
              });
            }}
          >
            <input
              className="text-input mono"
              type="password"
              autoComplete="off"
              aria-label={t("account.tokenLabel")}
              placeholder="ghp_… / github_pat_…"
              value={token}
              onChange={(event) => setToken(event.currentTarget.value)}
            />
            <button type="submit" className="button small primary" disabled={busy || !token.trim()}>
              {t("account.connect")}
            </button>
            <button type="button" className="button small ghost" onClick={() => onOpenUrl(NEW_TOKEN_URL)}>
              {t("account.newToken")}
            </button>
          </form>
        ) : null}
      </section>

      <section className="panel" aria-labelledby="settings-trash">
        <h2 id="settings-trash">{t("settings.trash")}</h2>
        <div className="setting-row">
          <span className="setting-label">
            {!trash || trash.entries === 0
              ? t("settings.trashEmpty")
              : t("settings.trashSummary", { entries: trash.entries, size: formatBytes(trash.totalBytes) })}
            {trash && trash.entries > 0 ? (
              <small>{t("settings.trashDetail", { purgeable: trash.purgeableEntries, days: TRASH_PURGE_DAYS })}</small>
            ) : null}
          </span>
          {trash && trash.purgeableEntries > 0 ? (
            confirming ? (
              <span className="confirm-group">
                <span className="danger-text small">{t("settings.irreversible")}</span>
                <button type="button" className="button small" onClick={() => setConfirming(false)}>
                  {t("actions.cancel")}
                </button>
                <button type="button" className="button small danger" onClick={() => void purge()}>
                  {t("settings.purgeConfirm", { count: trash.purgeableEntries })}
                </button>
              </span>
            ) : (
              <button type="button" className="button small" disabled={purging} onClick={() => setConfirming(true)}>
                {t("settings.purge", { days: TRASH_PURGE_DAYS })}
              </button>
            )
          ) : null}
        </div>
      </section>

      <section className="panel" aria-labelledby="settings-advanced">
        <h2 id="settings-advanced">{t("settings.advanced")}</h2>
        <div className="setting-row">
          <span className="setting-label">{t("settings.version")}</span>
          <span className="muted">Skills Manage {appStatus?.version ?? ""}</span>
        </div>
      </section>
    </div>
  );
}
