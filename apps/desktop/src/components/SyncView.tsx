import { RefreshCw } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";
import type { SkillUpdateStatus, UpdateState } from "../lib/api";
import { SkillTile } from "./ui";

type Props = {
  t: TFunction;
  locale: string;
  updates: SkillUpdateStatus[] | null;
  checkedAt: Date | null;
  checking: boolean;
  updatingPath: string | null;
  libraryCount: number;
  onCheck: () => void;
  onUpdate: (status: SkillUpdateStatus) => void;
  onExportAll: () => void;
};

const stateLabel: Record<
  UpdateState,
  "updates.available" | "updates.detected" | "updates.upToDate" | "updates.unavailable" | "updates.rateLimited" | "updates.error"
> = {
  "update-available": "updates.available",
  "source-detected": "updates.detected",
  "up-to-date": "updates.upToDate",
  "source-unavailable": "updates.unavailable",
  "rate-limited": "updates.rateLimited",
  error: "updates.error",
};

export function SyncView({ t, locale, updates, checkedAt, checking, updatingPath, libraryCount, onCheck, onUpdate, onExportAll }: Props) {
  return (
    <div className="page-body narrow">
      <section className="panel" aria-labelledby="updates-heading">
        <div className="panel-head">
          <h2 id="updates-heading">{t("updates.title")}</h2>
          {checkedAt ? (
            <span className="muted small">
              {t("updates.lastChecked", {
                time: checkedAt.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" }),
              })}
            </span>
          ) : null}
          <button type="button" className="button small" disabled={checking || updatingPath !== null} onClick={onCheck}>
            <RefreshCw className={checking ? "spin" : ""} size={13} strokeWidth={2} aria-hidden="true" />
            {checking ? t("updates.checking") : t("updates.check")}
          </button>
        </div>

        {updates && updates.length === 0 ? <p className="field-hint">{t("updates.none")}</p> : null}
        {updates && updates.length > 0 ? (
          <ul className="update-list">
            {updates.map((status) => (
              <li key={status.libraryPath} className={`update-row ${status.hasUpdate ? "highlight" : ""}`}>
                <SkillTile name={status.skillName} />
                <span className="update-main">
                  <strong>{status.skillName}</strong>
                  <span className="mono small muted">
                    {status.url.replace(/^https?:\/\/github\.com\//, "")} ·{" "}
                    {status.state === "source-detected" ? t("updates.notLinked") : status.current}
                    {status.hasUpdate && status.latest ? ` → ${status.latest}` : ""}
                  </span>
                </span>
                {status.hasUpdate ? (
                  <>
                    <span className="muted small">
                      {status.state === "source-detected" ? t("updates.detected") : t("updates.refreshHint")}
                    </span>
                    <button
                      type="button"
                      className="button primary small"
                      disabled={updatingPath !== null}
                      onClick={() => onUpdate(status)}
                    >
                      {updatingPath === status.libraryPath ? t("updates.updating") : t("updates.update")}
                    </button>
                  </>
                ) : (
                  <span className="muted small" title={status.message || undefined}>
                    {t(stateLabel[status.state])}
                  </span>
                )}
              </li>
            ))}
          </ul>
        ) : null}
        <p className="field-hint">{t("updates.hint")}</p>
      </section>

      <section className="panel" aria-labelledby="export-heading">
        <h2 id="export-heading">{t("export.title")}</h2>
        <div className="row-between">
          <span className="muted">{t("export.body", { count: libraryCount })}</span>
          <button type="button" className="button" onClick={onExportAll}>
            {t("export.button")}
          </button>
        </div>
      </section>
    </div>
  );
}
