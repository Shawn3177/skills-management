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

## Local Reference Files

The files below are local references only and are intentionally ignored:

- `agent_project_process_reference.html`
- `preview.html`
