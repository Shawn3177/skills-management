# Skill Import Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow the app to safely import a discovered local skill into the shared `~/.skills-manage/library/` folder.

**Architecture:** Rust owns the filesystem write. The frontend sends the selected skill path to a Tauri command and then re-runs the existing scanner. The import command validates `SKILL.md`, creates the shared library folder if needed, copies the skill directory without destructive overwrites, excludes risky/generated folders, and returns a small result object for UI feedback.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vitest.

---

## File Structure

- Create `apps/desktop/src-tauri/src/library.rs`: shared-library path handling, safe recursive copy, conflict naming, and import tests.
- Modify `apps/desktop/src-tauri/src/lib.rs`: expose `import_skill_to_library`.
- Modify `apps/desktop/src/App.tsx`: enable the import action for non-library skills, show import progress/result, and refresh scan after import.
- Modify `apps/desktop/src/App.test.tsx`: mock `import_skill_to_library` then `scan_skills`.
- Modify `docs/superpowers/plans/2026-06-04-skill-import-library.md`: this plan.

## Safety Contract

- Source folders are never modified.
- Destination is always inside `data_root/library`.
- Existing destination folders are never overwritten.
- Conflicts are resolved by creating `name-copy-2`, `name-copy-3`, and so on.
- Export-sensitive or generated entries are excluded during import: `.git`, `.env`, `node_modules`, `dist`, `target`, `cache`, `.cache`.
- If the source path is already inside the shared library, the command returns `alreadyManaged` and does not copy.

## Task 1: Rust Import TDD

**Files:**
- Create: `apps/desktop/src-tauri/src/library.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Rust tests**

Add tests that prove:

- a folder containing `SKILL.md` is copied into `data_root/library`.
- `.env`, `.git`, `node_modules`, `dist`, and `target` are excluded.
- importing the same folder twice creates a copy folder instead of overwriting.
- a missing `SKILL.md` returns an error.

- [ ] **Step 2: Run Rust tests and verify they fail**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: tests fail because `library.rs` implementation does not exist.

- [ ] **Step 3: Implement import logic**

Implement:

- `ImportResult`
- `import_skill_to_library(source_path: String) -> Result<ImportResult, String>`
- `import_skill_to_library_with_root(source_path: &Path, data_root: &Path)`
- safe recursive copy with exclusion rules.
- unique destination naming for conflicts.

- [ ] **Step 4: Register the Tauri command**

Add `import_skill_to_library` to `tauri::generate_handler!`.

- [ ] **Step 5: Run Rust tests and verify they pass**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: import, scanner, and app status tests pass.

## Task 2: Frontend Import Action

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.css`
- Modify: `apps/desktop/src/App.test.tsx`

- [ ] **Step 1: Update frontend test**

Mock Tauri invoke so the first `scan_skills` call returns a Codex skill, `import_skill_to_library` returns an import result, and the second `scan_skills` call returns that skill from `Shared Library`.

- [ ] **Step 2: Run frontend test and verify it fails**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: fails because the import action is not wired.

- [ ] **Step 3: Use the UI skills before changing UI**

Read `ui-ux-pro-max` and `design-taste-frontend`. Preserve a compact operations-tool UI, clear status feedback, accessible buttons, and disabled destructive actions.

- [ ] **Step 4: Implement import UI**

Update `App.tsx` so:

- Import button is enabled only when selected skill exists and `source !== "Shared Library"`.
- On click, it calls `import_skill_to_library` with `sourcePath`.
- The app shows importing/result/error status.
- After successful import, it calls `scan_skills` again.
- Enable/disable/repair/export remain disabled.

- [ ] **Step 5: Run frontend tests and verify they pass**

Run:

```powershell
cd apps/desktop
npm test -- --run
```

Expected: all frontend tests pass.

## Task 3: Verify and Publish

**Files:**
- All touched files.

- [ ] **Step 1: Run full verification**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd src-tauri
cargo test
cargo check
```

- [ ] **Step 2: Confirm ignored local files**

Run:

```powershell
git status --short --ignored
```

Expected: local HTML files, `node_modules`, `dist`, `target`, and `.codex-logs` are ignored only.

- [ ] **Step 3: Commit, push, and open PR**

Run:

```powershell
git add apps/desktop docs/superpowers/plans/2026-06-04-skill-import-library.md
git commit -m "feat: import skills to library"
git push -u origin codex/skill-import-library
```

Open a draft PR with verification and safety notes.

## Self-Review

- Spec coverage: implements MVP import-to-shared-library behavior and prepares the later enable/disable step.
- Placeholder scan: no unfinished marker text or undefined implementation steps.
- Safety check: this introduces writes only to the managed data root and never modifies source skills.
