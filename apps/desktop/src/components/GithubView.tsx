import { ExternalLink, Plus } from "lucide-react";
import { useState } from "react";
import type { TFunction } from "../i18n/useLocale";
import type { GithubAccount, GithubCatalog } from "../lib/api";
import { folderName, tools, type SkillRecord, type ToolId } from "../lib/skills";
import type { AddRequest } from "./AddView";
import { MyGithubPicker } from "./MyGithubPicker";
import { Chip } from "./ui";

type Props = {
  t: TFunction;
  locale: string;
  account: GithubAccount | null;
  catalog: GithubCatalog | null;
  loading: boolean;
  library: SkillRecord[];
  working: boolean;
  onRefresh: () => void;
  onConnect: () => void;
  onOpenUrl: (url: string) => void;
  onSubmit: (request: AddRequest) => void;
};

/** The connected account's repositories and stars, with the skills in them. */
export function GithubView({
  t,
  locale,
  account,
  catalog,
  loading,
  library,
  working,
  onRefresh,
  onConnect,
  onOpenUrl,
  onSubmit,
}: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [enableIn, setEnableIn] = useState<Set<ToolId>>(new Set<ToolId>(["claude-code"]));
  const login = account?.login ?? catalog?.login ?? null;
  const profileUrl = login ? `https://github.com/${login}` : null;

  return (
    <>
      <div className="page-body narrow">
        {account?.connected && login ? (
          <section className="panel account-card" aria-label={t("account.title")}>
            {catalog?.avatarUrl ? <img className="avatar" src={catalog.avatarUrl} alt="" width={44} height={44} /> : null}
            <span className="card-main">
              <strong>{catalog?.name ?? login}</strong>
              <span className="muted small">@{login}</span>
            </span>
            {profileUrl ? (
              <button type="button" className="button small" onClick={() => onOpenUrl(profileUrl)}>
                <ExternalLink size={13} strokeWidth={2} aria-hidden="true" />
                {t("mine.profile")}
              </button>
            ) : null}
          </section>
        ) : null}

        <section className="panel" aria-label={t("mine.title")}>
          <MyGithubPicker
            t={t}
            locale={locale}
            mode="page"
            account={account}
            catalog={catalog}
            loading={loading}
            libraryFolders={new Set(library.map((skill) => folderName(skill.sourcePath).toLowerCase()))}
            selected={selected}
            onSelectedChange={setSelected}
            onRefresh={onRefresh}
            onConnect={onConnect}
            onOpenUrl={onOpenUrl}
          />
        </section>
      </div>

      {selected.size > 0 ? (
        <footer className="page-footer">
          <span className="muted small">{t("add.enableAfter")}</span>
          {tools.map((tool) => (
            <Chip
              key={tool.id}
              pressed={enableIn.has(tool.id)}
              onClick={() =>
                setEnableIn((current) => {
                  const next = new Set(current);
                  if (!next.delete(tool.id)) {
                    next.add(tool.id);
                  }
                  return next;
                })
              }
            >
              {enableIn.has(tool.id) ? `✓ ${tool.name}` : tool.name}
            </Chip>
          ))}
          <span className="spacer" />
          <button type="button" className="button ghost" onClick={() => setSelected(new Set())}>
            {t("mine.clearSelection")}
          </button>
          <button
            type="button"
            className="button primary"
            disabled={working}
            onClick={() => onSubmit({ kind: "github-many", urls: [...selected], enableIn: [...enableIn] })}
          >
            <Plus size={15} strokeWidth={2.4} aria-hidden="true" />
            {working ? t("add.working") : t("add.submitCount", { count: selected.size })}
          </button>
        </footer>
      ) : null}
    </>
  );
}
