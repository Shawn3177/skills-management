# Skills Management

Skills Manage is a Windows-first desktop app for managing Agent Skills across Codex, Claude Code, and VS Code extensions.

The project is being built as a CC Switch-like local control surface: one shared skills library, safe enable/disable switches per tool, import/export through `.skillpack`, and conservative filesystem writes handled by the Rust backend.

## Tech Stack

- Desktop shell: Tauri 2.
- Frontend: React + TypeScript + Vite.
- Styling: Tailwind CSS with compact product UI.
- Backend: Rust commands through Tauri.
- Storage plan: SQLite plus `%USERPROFILE%\.skills-manage\settings.json`.
- Distribution plan: Windows MSI and portable ZIP first.

## Local Development

```powershell
cd apps/desktop
npm install
npm run dev
npm run tauri dev
```

Frontend checks:

```powershell
cd apps/desktop
npm test -- --run
npm run build
```

Windows packaging:

```powershell
cd apps/desktop
npm run tauri build
npm run package:portable
```

The Tauri build creates installer bundles under `apps/desktop/src-tauri/target/release/bundle/`.
The portable script creates a ZIP under `apps/desktop/src-tauri/target/release/bundle/portable/`.

Rust checks:

```powershell
cd apps/desktop/src-tauri
cargo test
cargo check
```

On Windows, `build.rs` embeds `src-tauri/windows-app-manifest.xml` (Common Controls v6) into every target, including the `cargo test` executables. Without it the test binaries exit at startup with `STATUS_ENTRYPOINT_NOT_FOUND`.

## Runtime Notes

- GitHub import and update checks use the unauthenticated API (60 requests/hour). Set `SKILLS_MANAGE_GITHUB_TOKEN` (or `GITHUB_TOKEN` / `GH_TOKEN`) to raise the limit; the token is only sent to `api.github.com`.
- `SKILLS_MANAGE_DATA_ROOT` overrides the data root (default `%USERPROFILE%\.skills-manage`).
- Disabled target copies and replaced library versions move to `<data root>\trash`. Settings shows the trash size and can permanently clear items older than 30 days after an explicit confirmation.

## Local Reference Files

The files below are local references only and are intentionally ignored:

- `agent_project_process_reference.html`
- `preview.html`
