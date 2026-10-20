import { useEffect, useState } from "react";
import { locales, type Locale } from "../i18n/messages";
import type { TFunction } from "../i18n/useLocale";
import { api, errorText, formatBytes, type AppStatus, type TrashSummary } from "../lib/api";
import type { Notice } from "./ui";

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
};

export function SettingsView({ t, locale, onLocaleChange, appStatus, onOpenFolder, onNotice }: Props) {
  const [trash, setTrash] = useState<TrashSummary | null>(null);
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
          <span className="setting-label">
            {t("settings.token")}
            <small>{t("settings.tokenHint")}</small>
          </span>
          <span className={`tag ${appStatus?.githubTokenConfigured ? "success" : "neutral"}`}>
            {appStatus?.githubTokenConfigured ? t("settings.tokenSet") : t("settings.tokenUnset")}
          </span>
        </div>
        <div className="setting-row">
          <span className="setting-label">{t("settings.version")}</span>
          <span className="muted">Skills Manage {appStatus?.version ?? ""}</span>
        </div>
      </section>
    </div>
  );
}
