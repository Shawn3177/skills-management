# Skills Manage v1 Technical Design

## Decision

Skills Manage v1 will be built as a Tauri 2 desktop app with a React + TypeScript frontend and a Rust backend.

The product should feel closer to CC Switch than to a web dashboard: a lightweight local control surface with a main window, tray entry, installer, portable build, local database, backups, and safe writes into existing AI coding tool folders.

## Goals

- Provide a public-installable desktop app for managing Agent Skills across Codex, Claude Code, and VS Code extensions.
- Keep one local shared library of skills and expose selected skills to each supported tool.
- Avoid corrupting user files through backup, validation, atomic write patterns, and conservative deletion.
- Start Windows-first while keeping the architecture portable to macOS and Linux.

## Non-Goals

- No cloud sync in v1.
- No online marketplace in v1.
- No account system in v1.
- No provider switching, proxy, usage dashboard, or session manager in v1.
- No promise that every VS Code extension has identical enable/disable semantics.

## Technology Stack

- Desktop shell: Tauri 2.
- Frontend: React, TypeScript, Vite.
- Styling and components: Tailwind CSS, Radix UI or shadcn-style components, lucide-react.
- Backend: Rust commands exposed to the frontend through Tauri.
- Storage: SQLite for business state; `settings.json` for device-level preferences.
- Packaging: Tauri bundle, Windows MSI, and portable ZIP.
- Tray: Tauri tray icon.
- Updates: reserve `tauri-plugin-updater`.
- Deep links: reserve `skillsmanage://`.

## Data Locations

- Data root: `~/.skills-manage/`.
- Windows data root: `%USERPROFILE%\.skills-manage\`.
- Library: `~/.skills-manage/library/`.
- Database: `~/.skills-manage/skills-manage.db`.
- Settings: `~/.skills-manage/settings.json`.
- Backups: `~/.skills-manage/backups/`.
- Trash: `~/.skills-manage/trash/`.
- Package staging: `~/.skills-manage/tmp/`.

This mirrors the local-control-console pattern used by CC Switch while staying specific to skills management.

## Architecture

The frontend is a renderer-only UI. It never imports Node-style filesystem APIs and never writes user files directly. It calls typed Tauri commands such as `scan_targets`, `import_skill`, `enable_skill`, `disable_skill`, `export_skillpack`, and `repair_skill`.

The Rust backend owns all filesystem behavior. It scans target folders, parses `SKILL.md`, computes hashes, copies directories, creates links, writes backups, imports and exports `.skillpack`, and records state in SQLite.

Each supported tool is implemented through a `ToolAdapter` boundary. Adding a new target tool should mean adding an adapter rather than changing the central library manager.

## Core Modules

### Frontend

- `AppShell`: layout, theme, window state, route-level navigation.
- `SkillsList`: skill list, search, filters, duplicate indicators.
- `SkillDetail`: metadata, health state, files, enabled targets, actions.
- `ImportExportPanel`: import folder, import package, export package.
- `SettingsPanel`: library path, target paths, link mode, backup settings.
- `TrayBridge`: reacts to refresh and open-window events triggered from the tray.

### Rust Backend

- `adapter`: Codex, Claude Code, and VS Code extension adapters.
- `library`: shared library import, copy, rename, soft delete.
- `pack`: `.skillpack` import/export and checksum validation.
- `storage`: SQLite schema, queries, transactions, migrations.
- `backup`: automatic backup, rotation, restore.
- `fs_ops`: path handling, symlink/junction/copy fallback, atomic writes.
- `health`: broken link, missing `SKILL.md`, invalid frontmatter, hash drift checks.

## Target Tool Adapters

### Codex

- Scan `$HOME/.agents/skills`.
- Scan `%USERPROFILE%\.codex\skills` on Windows when present.
- Enable by link when possible, copy fallback when link creation fails.

### Claude Code

- Scan common Claude Code skills paths.
- Preserve existing user-created skill directories unless explicitly imported.
- Enable by managed link or managed copy.

### VS Code Extensions

- v1 supports presets plus manual paths.
- The adapter treats each configured path as a target skills directory.
- Enable/disable semantics are conservative because extension behavior may differ.

## Skill Lifecycle

1. Scan target folders and find directories containing `SKILL.md`.
2. Parse frontmatter for `name` and `description`.
3. Compute a directory hash excluding ignored files.
4. Show discovered skills and duplicate state.
5. Import selected skill into `~/.skills-manage/library/`.
6. Enable selected target tools through link or copy.
7. Track enabled state and health in SQLite.
8. Disable by removing only managed links or managed copies.
9. Soft delete by moving library entries to trash.
10. Export one or more skills as `.skillpack`.

## Safety Model

- The app must never delete non-managed user files.
- Every write to a target tool folder creates a backup record first.
- Hard delete requires explicit confirmation and only applies to library-owned files.
- `.skillpack` export excludes `.git`, `.env`, credentials, caches, `node_modules`, and build outputs by default.
- Link creation failures fall back to managed copies rather than requiring admin rights.
- Health checks distinguish between external user files and files managed by Skills Manage.

## Public Types

### `SkillRecord`

- `id`
- `name`
- `description`
- `sourceKind`
- `sourcePath`
- `libraryPath`
- `hash`
- `createdAt`
- `updatedAt`
- `enabledTargets`
- `healthStatus`
- `supportFiles`

### `ToolAdapter`

- `id`
- `displayName`
- `platforms`
- `candidatePaths`
- `scan()`
- `enable(skill, mode)`
- `disable(skill)`
- `validateTarget()`
- `repair(skill)`

### `.skillpack/manifest.json`

- `formatVersion`
- `packageId`
- `createdAt`
- `skills[]`
- `checksums`
- `targetStates`
- `exclusions`
- `sourceAppVersion`

## MVP

The first implementation should not attempt to clone all CC Switch capabilities. The smallest valuable version is:

1. Tauri app shell with main window and tray.
2. SQLite and local data root initialization.
3. Scan Codex, Claude Code, and manual VS Code skill paths.
4. Parse `SKILL.md` and show skill metadata.
5. Import skills into the shared library.
6. Enable or disable skills for Codex and Claude Code.
7. Show health checks for broken links and missing `SKILL.md`.
8. Export and import `.skillpack`.
9. Build Windows MSI and portable ZIP.

## Testing Strategy

- Rust unit tests for path normalization, ignored-file rules, hash generation, adapter scan behavior, link fallback, backup restore, and package validation.
- Frontend unit tests for list filtering, target toggles, import/export state, and error messages.
- Integration tests using temporary directories to simulate Codex, Claude Code, and VS Code target folders.
- Packaging smoke test for Windows MSI and portable ZIP.

## Distribution Plan

- Start with Windows 10+.
- Publish unsigned development builds while iterating.
- For public stable release, add Windows code signing to reduce trust warnings.
- Publish MSI and portable ZIP from GitHub Releases.
- Add updater metadata after manual releases are stable.

## Open Risks

- Claude Code and Codex skills paths may change, so adapters need easy path overrides.
- VS Code extension support may vary by extension and should remain preset/manual in v1.
- Windows symlink creation may fail without permission; copy fallback must be first-class.
- Public installers without code signing may trigger warnings until signing is set up.

## References

- CC Switch product pattern: https://ccswitch.ai/
- CC Switch repository: https://github.com/farion1231/cc-switch
- Tauri 2 docs: https://v2.tauri.app/
- Codex Agent Skills: https://developers.openai.com/codex/skills
- Claude Code Skills: https://code.claude.com/docs/en/skills
- Agent Skills specification: https://agentskills.io/specification
