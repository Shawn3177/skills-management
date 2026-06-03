# AGENTS.md

## Project

Skills Manage is a Windows-first desktop app for managing Agent Skills across Codex, Claude Code, and VS Code extensions. It should feel like a lightweight local control surface similar to CC Switch, not like a generic web dashboard.

The v1 scope is local-first skills management:

- Keep one shared local skills library.
- Scan existing skill folders.
- Import skills into the shared library.
- Enable or disable skills for supported tools.
- Export and import `.skillpack` packages.
- Keep user files safe through backups, conservative writes, and soft deletion.

## Tech Stack

- Desktop shell: Tauri 2.
- Frontend: React + TypeScript + Vite.
- Backend: Rust.
- Local data: SQLite plus `settings.json`.
- Styling: Tailwind CSS with accessible, restrained product UI.
- Icons: use a consistent vector icon family. Do not use emoji as interface icons.
- Packaging: Windows MSI and portable ZIP first.

## Repository Layout

Planned structure:

```text
.
├─ apps/
│  └─ desktop/
│     ├─ src/
│     └─ src-tauri/
├─ docs/
│  ├─ development-workflow.md
│  ├─ skills-manage-v1-plan.md
│  └─ superpowers/
├─ .github/
│  ├─ pull_request_template.md
│  └─ workflows/
├─ AGENTS.md
└─ README.md
```

Current repository may only contain docs until the Tauri app skeleton is created.

## Local Files That Must Stay Local

These HTML files are local references only and must not be pushed:

- `agent_project_process_reference.html`
- `preview.html`

They are intentionally ignored by `.gitignore`.

## Development Workflow

- Do not build long-running changes directly on `main`.
- Create a feature branch for each unit of work, for example `feat/tauri-shell` or `chore/repo-workflow-assets`.
- Keep changes scoped. Do not mix product code, docs, and unrelated cleanup in one commit unless the task explicitly asks for it.
- Before implementation, read the relevant plan or spec under `docs/`.
- For complex work, write or update an implementation plan before coding.
- After code changes, run the most specific verification command available.
- Review diffs before committing.

## Commands

Before the app skeleton exists:

```powershell
git status --short --branch --ignored
rg -n "TODO[:]|TBD[:]" AGENTS.md .github docs -g "!docs/superpowers/plans/**" -g "!.github/workflows/ci.yml"
```

After `apps/desktop` exists:

```powershell
cd apps/desktop
npm install
npm run build
npm run tauri dev
```

Rust checks should run from the Tauri project once `src-tauri` exists:

```powershell
cd apps/desktop/src-tauri
cargo test
cargo check
```

## Frontend Design Rules

When creating or changing frontend UI, use these installed design skills before implementation:

- `ui-ux-pro-max`
- `design-taste-frontend` from the local `taste-skill`

Also use Product Design `get-context` when a screen, prototype, redesign, or visual direction needs a design brief.

Product UI direction:

- This is an operational desktop tool. Prefer dense but calm layouts over marketing-style sections.
- Use predictable navigation, clear lists, tables, filters, empty states, loading states, and error states.
- Keep cards shallow and purposeful. Do not nest cards.
- Use icons from one vector family. Avoid emoji as structural UI.
- Maintain accessible contrast, visible focus states, keyboard navigation, and responsive behavior.
- Do not create decorative gradients or visual noise unless the design brief explicitly calls for it.

## Safety Rules

- Never delete or overwrite user-managed skill folders silently.
- Only remove files known to be managed by Skills Manage.
- Back up target tool folders before write operations.
- Treat symlink and junction creation failures as expected on Windows and provide copy fallback.
- Never export `.env`, credentials, caches, `node_modules`, `.git`, or build outputs in `.skillpack`.
- Keep dangerous filesystem operations inside Rust backend code, not frontend UI code.

## Pull Request Expectations

Every pull request should include:

- What changed.
- Why it changed.
- Verification commands and results.
- Screenshots or short notes for UI changes.
- Risks, especially around filesystem writes or packaging.
- Confirmation that ignored local HTML references are not included.
