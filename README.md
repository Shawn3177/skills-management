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
A local build is unsigned and has no updater files; releases are built by CI.

Run the build from a normal terminal. Inside the Claude desktop app, which is an MSIX package, writes to `%LOCALAPPDATA%` are redirected. Tauri moves its downloaded NSIS toolset across that boundary, so the NSIS step fails with `os error 17`.

## Releasing

Installed copies check `https://github.com/Shawn3177/skills-management/releases/latest/download/latest.json` on launch. They can also check from Settings > Version. A new version is installed with the per-user setup (NSIS, passive mode), and the app then restarts. Only signed builds are accepted.

One-time setup:

1. Generate the signing key pair: `npx tauri signer generate -w ~/.tauri/skills-manage.key` (run it in `apps/desktop`). The public key is in `plugins.updater.pubkey` in `tauri.conf.json`.
2. Store the private key as the `TAURI_SIGNING_PRIVATE_KEY` repository secret. If the key has a password, also store it as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The current key has no password; it was generated with `--ci`. Signing does not prompt when `CI=true`, which GitHub Actions sets.
3. Back up the private key. Without it, installed copies cannot verify a new version and must be reinstalled by hand.

For each release:

1. Set the same version in `apps/desktop/package.json` (`npm version <x.y.z> --no-git-tag-version`), `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`. Merge that to `main`.
2. Tag `main` with `v<x.y.z>` and push the tag. `.github/workflows/release.yml` checks that the tag matches the version, runs the tests, then publishes the release with these files:
   - `setup.exe`
   - `.msi`
   - `.sig` signatures
   - `latest.json`
   - the portable ZIP

The portable ZIP is not updated in place. Its update prompt installs the setup version instead.

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
