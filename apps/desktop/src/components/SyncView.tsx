import { Download, RefreshCw } from "lucide-react";
import type { TFunction } from "../i18n/useLocale";
import type { ProgramUpdate, SkillUpdateStatus, UpdateState } from "../lib/api";
import { githubSource, type SkillRecord } from "../lib/skills";
import { SkillTile } from "./ui";

/** Where "Update all" is: the `index`-th of `total`, updating `skill`. */
export type BulkProgress = { index: number; total: number; skill: string };

/** `updatingPath` while a skill's update command runs. */
export function programKey(skillName: string): string {
  return `program:${skillName}`;
}

type Props = {
  t: TFunction;
  locale: string;
  updates: SkillUpdateStatus[] | null;
  checkedAt: Date | null;
  checking: boolean;
  updatingPath: string | null;
  /** Skills their own program updates. */
  programs: ProgramUpdate[];
  bulk: BulkProgress | null;
  libraryCount: number;
  /** Shared-library skills with a GitHub source (listed before the first check). */
  sources: SkillRecord[];
  /** Skills outside the library whose source is known. */
  waiting: number;
  onCheck: () => void;
  onUpdateAll: () => void;
  onUpdate: (status: SkillUpdateStatus) => void;
  onRunProgram: (program: ProgramUpdate) => void;
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

export function SyncView({
  t,
  locale,
  updates,
  checkedAt,
  checking,
  updatingPath,
  programs,
  bulk,
  libraryCount,
  sources,
  waiting,
  onCheck,
  onUpdateAll,
  onUpdate,
  onRunProgram,
  onExportAll,
}: Props) {
  const busy = checking || updatingPath !== null;
  const nothingToUpdate = (updates ?? sources).length === 0 && programs.length === 0;
  return (
    <div className="page-body narrow">
      <section className="panel" aria-labelledby="updates-heading">
        <div className="panel-head">
          <h2 id="updates-heading">{t("updates.title")}</h2>
          <span className="muted small" role="status" aria-live="polite">
            {bulk
              ? t("updates.allProgress", { index: bulk.index, total: bulk.total, skill: bulk.skill })
              : checkedAt
                ? t("updates.lastChecked", {
                    time: checkedAt.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" }),
                  })
                : null}
          </span>
          <button type="button" className="button small" disabled={busy} onClick={onCheck}>
            <RefreshCw className={checking && !bulk ? "spin" : ""} size={13} strokeWidth={2} aria-hidden="true" />
            {checking ? t("updates.checking") : t("updates.check")}
          </button>
          <button
            type="button"
            className="button small primary"
            disabled={busy || nothingToUpdate}
            title={t("updates.allHint")}
            onClick={onUpdateAll}
          >
            <Download size={13} strokeWidth={2} aria-hidden="true" />
            {t("updates.all")}
          </button>
        </div>

        {nothingToUpdate ? <p className="field-hint">{t("updates.none")}</p> : null}
        {!updates && sources.length > 0 ? (
          <ul className="update-list">
            {sources.map((skill) => (
              <li key={skill.sourcePath} className="update-row">
                <SkillTile name={skill.name} />
                <span className="update-main">
                  <strong>{skill.name}</strong>
                  <span className="mono small muted">{(githubSource(skill) ?? "").replace(/^https?:\/\/github\.com\//, "")}</span>
                </span>
                <span className="muted small">{t("updates.notChecked")}</span>
              </li>
            ))}
          </ul>
        ) : null}
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
        {programs.length > 0 ? (
          <>
            <h3 className="update-group">{t("updates.programs")}</h3>
            <ul className="update-list">
              {programs.map((program) => (
                <li key={program.skillName} className="update-row">
                  <SkillTile name={program.skillName} />
                  <span className="update-main">
                    <strong>{program.skillName}</strong>
                    <span className="mono small muted">{program.command}</span>
                  </span>
                  <span className="muted small">{t("updates.programHint")}</span>
                  <button
                    type="button"
                    className="button small"
                    disabled={busy}
                    aria-label={t("updates.runFor", { skill: program.skillName, command: program.command })}
                    onClick={() => onRunProgram(program)}
                  >
                    {updatingPath === programKey(program.skillName) ? t("updates.updating") : t("updates.run")}
                  </button>
                </li>
              ))}
            </ul>
          </>
        ) : null}
        <p className="field-hint">
          {t("updates.hint")}
          {waiting > 0 ? ` ${t("updates.waiting", { count: waiting })}` : ""}
        </p>
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
