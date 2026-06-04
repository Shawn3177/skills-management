# Local Skill Scanner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first real local skill scanner so the desktop shell can show skills discovered from common Codex, Claude Code, Agent Skills, and shared-library folders.

**Architecture:** Rust owns filesystem reads and exposes a `scan_skills` Tauri command. The scanner is read-only: it discovers directories containing `SKILL.md`, extracts simple metadata, and returns records to React. The frontend keeps the existing compact shell but uses backend results when available, falling back to demo data only when the scan returns nothing.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vitest.

---

## File Structure

- Create `apps/desktop/src-tauri/src/scanner.rs`: read-only scanning, metadata parsing, path defaults, and unit tests.
- Modify `apps/desktop/src-tauri/src/lib.rs`: expose `scan_skills` and keep `get_app_status`.
- Modify `apps/desktop/src/lib/skills.ts`: align frontend types with backend scan payload while keeping list helpers.
- Modify `apps/desktop/src/App.tsx`: call `scan_skills`, show loading/error/empty states, and keep demo fallback.
- Modify `apps/desktop/src/App.test.tsx`: mock Tauri invoke and test the backend scan path.
- Create `docs/superpowers/plans/2026-06-04-local-skill-scanner.md`: this plan.

## Scope Boundaries

- This task is scan-only. It does not import, enable, disable, link, copy, delete, back up, or package any skill.
- Frontmatter parsing is intentionally minimal: read `name:` and `description:` from the top YAML-like block if present.
- The scanner handles missing directories as normal and returns an empty list rather than an error.
- Support files are summarized from the top level of each skill directory, excluding hidden entries.

## Task 1: Rust Scanner TDD

**Files:**
- Create: `apps/desktop/src-tauri/src/scanner.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write scanner tests**

Add tests in `scanner.rs` for:

- `scan_skill_roots` finds a directory containing `SKILL.md`.
- frontmatter `name` and `description` are extracted.
- missing roots return an empty result.

- [ ] **Step 2: Run Rust tests and verify scanner tests fail**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: tests fail because scanner implementation is missing.

- [ ] **Step 3: Implement the read-only scanner**

Implement:

- `ScannedSkill`
- `TargetState`
- `SkillHealth`
- `candidate_skill_roots()`
- `scan_skill_roots(roots: &[PathBuf])`
- `scan_default_skills()`
- `parse_skill_metadata(contents: &str)`

- [ ] **Step 4: Expose `scan_skills` command**

Register `scan_skills` in `tauri::generate_handler!`.

- [ ] **Step 5: Run Rust tests and verify they pass**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: scanner and status tests pass.

## Task 2: Frontend Type Alignment and Backend Loading

**Files:**
- Modify: `apps/desktop/src/lib/skills.ts`
- Modify: `apps/desktop/src/data/demoSkills.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.test.tsx`

- [ ] **Step 1: Update tests to mock backend scan**

Update `App.test.tsx` to mock `@tauri-apps/api/core` and resolve `scan_skills` with a scanned skill named `local-scan-skill`.

- [ ] **Step 2: Run frontend tests and verify they fail**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: test fails because `App.tsx` does not call `scan_skills` yet.

- [ ] **Step 3: Update the app to load backend scan results**

Implement a React effect that calls `invoke<SkillRecord[]>("scan_skills")`, shows a loading state while scanning, uses scanned records when any are returned, and keeps demo records only as an empty-machine fallback.

- [ ] **Step 4: Preserve UI quality**

Use `ui-ux-pro-max` and `design-taste-frontend` before adjusting UI. Keep:

- compact desktop layout.
- visible scan status.
- non-destructive disabled action buttons.
- clear empty state for no discovered skills.

- [ ] **Step 5: Run frontend tests and verify they pass**

Run:

```powershell
cd apps/desktop
npm test -- --run
```

Expected: app and helper tests pass.

## Task 3: Verify, Commit, Push, PR

**Files:**
- All touched files.

- [ ] **Step 1: Run full local verification**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd src-tauri
cargo test
cargo check
```

- [ ] **Step 2: Confirm local-only files stay ignored**

Run:

```powershell
git status --short --ignored
```

Expected: local HTML files and build outputs appear only as ignored.

- [ ] **Step 3: Commit and push**

Run:

```powershell
git add apps/desktop docs/superpowers/plans/2026-06-04-local-skill-scanner.md
git commit -m "feat: scan local skills"
git push -u origin codex/skill-scanner
```

- [ ] **Step 4: Open a draft PR**

Create a draft PR to `main` and include verification commands and scan-only safety notes.

## Self-Review

- Spec coverage: implements MVP item 3 and begins item 4 by detecting `SKILL.md` and reading basic metadata.
- Placeholder scan: no unfinished marker text or undefined task steps.
- Safety check: no write operations are introduced; all filesystem behavior is read-only.
