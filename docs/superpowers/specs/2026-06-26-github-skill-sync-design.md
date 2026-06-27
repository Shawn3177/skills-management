# GitHub skill sync: import + check/update

## Context

Skills today are imported from local folders; the library doesn't record any origin. The user wants to import skills from **GitHub** and later check the GitHub origin for newer commits and selectively update. This is the app's **first network capability** (the v1 plan was local-only), so security and correctness matter more than usual.

This design is hardened from a code-grounded QA review. Two original mechanisms were impossible against the existing code and are corrected here:
- "Update by reusing the import path" — impossible: `library::unique_destination` only ever returns a NEW folder (`-copy-N`), never an existing one. Update needs a dedicated in-place replace.
- "Re-sync enabled copies via disable+re-enable" — unsound: `enable` refuses to overwrite a managed folder, and the flow had no way to know which targets were enabled. Corrected to: compute enabled targets first, replace library in place (markers stay valid), then force-refresh each enabled managed copy.

Built in **two PRs**: (1) Import from GitHub; (2) Check + selective update.

## Decisions

- Source = GitHub, **public repos only** (unauthenticated API, ~60 req/hr). Private/token = future.
- A skill may be a whole repo (root `SKILL.md`) or a subdir (`/tree/<ref>/<subdir>`).
- Version identity = the **path-filtered latest commit SHA** for the subdir, stored at import and compared on check using the **same** query (consistent, so no always-shows-update). A moved/deleted subdir surfaces as a distinct "source unavailable" state, not silent "up to date". (Subdir tree SHA is a more-precise future refinement.)
- Manual check only (no polling).
- Source metadata is **not portable**: `.skills-manage-source.json` (and the existing `.skills-manage-link.json`) are excluded from `.skillpack` export and from library copies.

## Source metadata

Each GitHub-sourced library skill gets `.skills-manage-source.json` in its dir:
```json
{ "kind": "github", "owner": "...", "repo": "...", "ref": "main",
  "subdir": "skills/foo", "syncedCommit": "<sha>", "syncedAt": <unix ms>,
  "url": "https://github.com/owner/repo/tree/main/skills/foo" }
```
`ref` is always the **resolved** branch (default branch looked up if the URL had none). The library folder name is derived by `safe_folder_name` and may differ from `subdir`; check/update must locate the dir by **scanning for this file**, never by reconstructing the name.

## PR #1 — Import from GitHub

New Rust dep: a lightweight blocking HTTP client (`ureq` with rustls).

New module `src-tauri/src/github.rs`:
- **URL parsing** (`parse_github_url`): accept `https://github.com/<owner>/<repo>`, `.../tree/<ref>/<subdir...>`, tolerate trailing `.git`/slashes. Reject any host other than `github.com`/`www.github.com`. Returns `{ owner, repo, ref: Option, subdir: String }`. Pure function → unit-tested.
- **Host pinning / SSRF guard**: only ever call `https://api.github.com/...` and `https://codeload.github.com/...`, reconstructed from the parsed `owner/repo/ref`. Constrain redirects (small cap). Never pass the user URL through to the client.
- **Default-branch resolution**: if `ref` is None, `GET https://api.github.com/repos/{owner}/{repo}` → `default_branch`.
- **Latest commit for path** (`latest_commit`): `GET /repos/{owner}/{repo}/commits?path={subdir}&sha={ref}&per_page=1` → first `sha`. Empty array ⇒ "source not found at path" error.
- **Download + safe extract** (`fetch_subdir_into`): `GET https://codeload.github.com/{owner}/{repo}/zip/{ref}` → bytes → open with the existing `zip` crate. Read the **actual top-level folder** from entry 0's first path component (do NOT string-build `repo-ref`). For each entry under `<top>/<subdir>/`, re-apply a path-safety check and write into a staging dir. Validate `SKILL.md` exists at the subdir root, else error (whole-repo import requires a root `SKILL.md`, else guide to a `/tree` URL).
- Guards: factor `is_safe_relative` out of `pack.rs` into `fs_ops` and apply to **every** zip entry; cap total uncompressed bytes and entry count (zip-bomb).

Backend command `import_from_github(url) -> { skillName, libraryPath, message }` (lib.rs):
- Parse + validate URL → resolve ref → resolve commit SHA → download+extract to staging → validate → place into library via `unique_destination` (new skill, save-as-copy on name conflict) → write `.skills-manage-source.json` (with resolved ref + commit) → clean staging.

Exclusion fix: add `.skills-manage-source.json` and `.skills-manage-link.json` to the entries skipped by `pack::add_dir_to_zip` (export) and `fs_ops::copy_skill_dir` (so source/link files never leak into skillpacks or copies). Keep them out of `is_excluded_entry`'s general list only if needed; simplest is a dedicated `is_internal_marker(name)` checked alongside `is_excluded_entry` in those two copy/zip walkers.

Frontend: an "Import from GitHub" control in the Packages tab — a URL text input + button → `invoke("import_from_github", { url })` → status via `StatusMessage` → `loadSkills()`. New i18n keys.

Tests (PR #1):
- Rust: `parse_github_url` variants (repo, tree+subdir, `.git`, bad host rejected); safe-extract rejects `../` entries and over-cap archives; `fetch_subdir_into` extracts a fixture zip's subdir and finds `SKILL.md` (feed bytes, no network). Keep the HTTP calls behind a thin seam so the extract/validate logic is testable without network.
- Frontend: mock `invoke`; entering a URL + clicking calls `import_from_github` and shows the result.

## PR #2 — Check + selective update

- `check_skill_updates() -> [{ libraryPath, skillName, hasUpdate, current, latest, state, url }]`: scan library dirs for `.skills-manage-source.json`; for each, `latest_commit(owner, repo, ref, subdir)`; compare to `syncedCommit`. Keyed by **libraryPath** (unique). Read `X-RateLimit-Remaining/Reset`; on exhaustion return a clear "rate limited until HH:MM" state. Short-TTL cache so re-clicks don't re-spend. Moved/deleted subdir ⇒ `state: "source-unavailable"`.
- `update_skill_from_github(libraryPath) -> {...}`: read the dir's source file → download+validate the latest into a staging dir → **atomic swap**: `fs::rename` existing dir → `trash/`, then staging → the original path → rewrite source file with new commit. Compute **enabled targets up front** (scanner managed-copy detection); after the in-place swap (path unchanged ⇒ markers still match), force-refresh each enabled managed copy (new backend op that overwrites the managed copy content + rewrites marker). Transactional: if anything fails after the swap, surface partial-success clearly; never move the original aside before the replacement is validated on disk.
- Frontend: a "Check for updates" button; skills with `hasUpdate` show a badge and appear in a checklist with "Update selected"; `source-unavailable` shown distinctly.

## Security

- Host allowlist (github.com input only; api/codeload reconstructed from validated parts); redirect cap.
- Path-traversal check on every remote zip entry; uncompressed-size + entry-count caps.
- TLS via rustls. The app only stores files — the user is trusting the repo's content; surface the source URL prominently.

## Out of scope

Private repos/auth, background/auto checks, non-GitHub sources, release/semver versioning, proxy config.

## Verification

`cargo test` + `npm test -- --run` + `npm run build`; manual: import a real public skill repo/subdir, confirm it lands in the library with a source file; (PR #2) bump the repo, check shows an update, update replaces in place and refreshes enabled copies.
