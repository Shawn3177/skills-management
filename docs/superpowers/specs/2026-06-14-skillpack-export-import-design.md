# .skillpack export / import

## Context

Migrating skills between machines is the flagship v1 feature, but the UI only has "waiting for backend" placeholders (Packages tab + the per-skill "Export .skillpack" button). This wires up the real backend: export skills to a `.skillpack` file and import one on another machine to restore them into the shared library.

**Decisions (confirmed):**
- Export scope: **selected skill** (detail / Imported tab) **and whole library** (Packages tab). Both produce the same format with `skills[]` of length 1 or N.
- Import behavior: **into the shared library only** — no auto-enable. `targetStates` is recorded in the manifest for reference; the user enables via the Imported tab's one-click bulk enable.
- Conflict on import: **save-as-copy** (reuse existing library import behavior); no skip/overwrite prompt.

## Format

`.skillpack` is a ZIP containing:
- `manifest.json` at the root.
- `skills/<safe-name>/…` — each skill's full directory, excluding the entries in `is_excluded_entry` (`.git`, `.env`, `node_modules`, `dist`, `target`, `cache`, `.cache`).

`manifest.json`:
```json
{
  "formatVersion": 1,
  "packageId": "<timestamp-based id>",
  "createdAt": "<unix ms>",
  "sourceAppVersion": "0.1.0",
  "exclusions": [".git", ".env", "node_modules", "dist", "target", "cache", ".cache"],
  "skills": [
    { "name": "<name>", "path": "skills/<safe-name>", "targetStates": { "codex": true, "claude-code": false } }
  ],
  "checksums": { "skills/<safe-name>/SKILL.md": "<sha256>", "...": "..." }
}
```

## Backend — new `pack` module (`src-tauri/src/pack.rs`)

New deps: `zip`, `sha2`, `tauri-plugin-dialog`.

- `export_skillpack(sources: Vec<ExportSkillInput>, destination: String) -> ExportResult`
  - `ExportSkillInput = { source_path, name, targets: Vec<{id, enabled}> }` (frontend passes what it already knows; `targetStates` is informational).
  - For each source: validate it is a dir with `SKILL.md`; walk it (skipping excluded entries); add files to the zip under `skills/<safe-name>/…`; accumulate sha256 per file.
  - Write `manifest.json` last. Return `{ skillCount, destination }`.
- `import_skillpack(package_path: String) -> ImportPackResult`
  - Open the zip; parse + validate `manifest.json` (supported `formatVersion`); recompute each entry's sha256 and compare to `checksums` (mismatch → error).
  - Extract to a unique temp staging dir under the data root, then for each manifest skill call the existing `library::import_skill_to_library_with_root(staging/skills/<name>, data_root)` — this reuses SKILL.md validation, exclusions, and `unique_destination` (save-as-copy). Clean up the temp dir.
  - Return `{ imported, savedAsCopy, skipped, message }` with per-skill outcomes.

Reuse: `library::import_skill_to_library_with_root`, `library::is_excluded_entry` (make it `pub(crate)` or mirror the list), `default_data_root`.

`lib.rs`: `mod pack;`, register `tauri_plugin_dialog::init()`, add `export_skillpack` / `import_skillpack` to `generate_handler!`.

Capability: add `dialog:allow-open` and `dialog:allow-save` to the default capability file.

## Frontend

New dep: `@tauri-apps/plugin-dialog`.

- Replace the preview handlers with real ones:
  - Export (detail / Imported per-skill): `save({ filters: [{ name: "skillpack", extensions: ["skillpack"] }], defaultPath: "<name>.skillpack" })` → if path, `invoke("export_skillpack", { sources: [{ sourcePath, name, targets }], destination })`.
  - Export whole library (Packages): collect all shared-library grouped skills → same `export_skillpack` with N sources, `defaultPath: "skills-library.skillpack"`.
  - Import (Packages): `open({ filters: [...skillpack], multiple: false })` → `invoke("import_skillpack", { packagePath })` → status summary → `loadSkills()`.
- Status via existing `StatusMessage` / `utilityMessage`. New i18n keys: exporting/exported, importing/imported, plus error fallbacks.

## Tests

- Rust (`pack.rs`): export a temp skill → zip + manifest exist, excluded entries absent; round-trip (export → import into a fresh data root → skill in library with SKILL.md); checksum tamper → import errors; multi-skill pack imports all.
- Frontend (`App.test.tsx`): mock `@tauri-apps/api/core` invoke and `@tauri-apps/plugin-dialog` save/open; export button calls `export_skillpack` with chosen path; import calls `import_skillpack` and shows the summary; cancelled dialog (null path) does nothing.

## Out of scope (YAGNI)

Multi-select export UI, skip/overwrite import prompt, auto-restore enable states, signed/encrypted packs.

## Verification

`cargo test` (src-tauri), `npm test -- --run` + `npm run build` (apps/desktop); manual export→import round-trip in the app window.
