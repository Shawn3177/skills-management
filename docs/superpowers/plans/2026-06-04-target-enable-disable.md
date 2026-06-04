# Target Enable Disable Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow a shared-library skill to be enabled or disabled for Codex and Claude Code target folders without touching unmanaged user folders.

**Architecture:** Rust owns all filesystem writes through a new target-management module. The frontend sends the selected shared-library skill path, target id, and desired enabled state to a Tauri command, then re-runs the scanner. v1 uses managed copies with a `.skills-manage-link.json` marker instead of symlinks so Windows permission failures do not block the workflow; future settings can add symlink or junction mode.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vitest.

---

## File Structure

- Create `apps/desktop/src-tauri/src/targets.rs`: target profiles, managed marker handling, safe enable/disable copy operations, and unit tests.
- Modify `apps/desktop/src-tauri/src/scanner.rs`: derive target enabled state from managed markers for shared-library skills while preserving source-based enabled state for existing tool folders.
- Modify `apps/desktop/src-tauri/src/lib.rs`: expose `set_skill_target_enabled`.
- Modify `apps/desktop/src/App.tsx`: enable Codex and Claude Code target buttons for shared-library skills, show target action progress/result/error, and refresh scan after a toggle.
- Modify `apps/desktop/src/App.css`: add target action status styles and keep buttons accessible.
- Modify `apps/desktop/src/App.test.tsx`: cover a shared-library skill target toggle and scan refresh.
- Create `docs/superpowers/plans/2026-06-04-target-enable-disable.md`: this plan.

## Safety Contract

- Only skills whose `sourcePath` is inside `data_root/library` can be enabled into targets.
- Codex target id maps to `%USERPROFILE%\.codex\skills`.
- Claude Code target id maps to `%USERPROFILE%\.claude\skills`.
- VS Code remains visible but disabled until its adapter rules are specified.
- Enable creates the target root if missing, then copies the source skill into `target_root/<skill-folder-name>`.
- Enable never overwrites an existing target folder.
- If the destination already has a `.skills-manage-link.json` marker for the same source path, enable returns `alreadyEnabled`.
- If the destination exists without a valid marker, enable returns a conflict error and performs no write.
- Disable only removes folders with a valid `.skills-manage-link.json` marker.
- Disable moves managed folders to `data_root/trash/<target-id>-<skill-name>-<timestamp>` instead of hard deleting.
- Export-sensitive or generated entries are excluded during target copies: `.git`, `.env`, `node_modules`, `dist`, `target`, `cache`, `.cache`.

## Task 1: Rust Target Management TDD

**Files:**
- Create: `apps/desktop/src-tauri/src/targets.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Write failing Rust tests**

Add tests in `targets.rs` proving these behaviors:

```rust
#[test]
fn enables_shared_library_skill_as_managed_copy() {
    let data_root = unique_temp_dir("data-root-target-enable");
    let library_skill = create_library_skill(&data_root, "copy-me");
    let target_root = unique_temp_dir("codex-target-root");

    let result = set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &target_root).unwrap();

    assert!(result.enabled);
    assert!(result.changed);
    assert!(target_root.join("copy-me").join("SKILL.md").is_file());
    assert!(target_root.join("copy-me").join(".skills-manage-link.json").is_file());
}

#[test]
fn enable_refuses_to_overwrite_unmanaged_target_folder() {
    let data_root = unique_temp_dir("data-root-target-conflict");
    let library_skill = create_library_skill(&data_root, "conflict-skill");
    let target_root = unique_temp_dir("codex-target-conflict");
    fs::create_dir_all(target_root.join("conflict-skill")).unwrap();
    fs::write(target_root.join("conflict-skill").join("SKILL.md"), "# User folder\n").unwrap();

    let error = set_skill_target_enabled_with_root(&library_skill, "codex", true, &data_root, &target_root).unwrap_err();

    assert!(error.contains("not managed by Skills Manage"));
    assert_eq!(fs::read_to_string(target_root.join("conflict-skill").join("SKILL.md")).unwrap(), "# User folder\n");
}

#[test]
fn disables_managed_copy_by_moving_it_to_trash() {
    let data_root = unique_temp_dir("data-root-target-disable");
    let library_skill = create_library_skill(&data_root, "disable-me");
    let target_root = unique_temp_dir("claude-target-root");
    set_skill_target_enabled_with_root(&library_skill, "claude-code", true, &data_root, &target_root).unwrap();

    let result = set_skill_target_enabled_with_root(&library_skill, "claude-code", false, &data_root, &target_root).unwrap();

    assert!(!result.enabled);
    assert!(result.changed);
    assert!(!target_root.join("disable-me").exists());
    assert!(fs::read_dir(data_root.join("trash")).unwrap().count() >= 1);
}

#[test]
fn disable_refuses_unmanaged_target_folder() {
    let data_root = unique_temp_dir("data-root-target-disable-conflict");
    let library_skill = create_library_skill(&data_root, "user-owned");
    let target_root = unique_temp_dir("claude-target-conflict");
    fs::create_dir_all(target_root.join("user-owned")).unwrap();
    fs::write(target_root.join("user-owned").join("SKILL.md"), "# User folder\n").unwrap();

    let error = set_skill_target_enabled_with_root(&library_skill, "claude-code", false, &data_root, &target_root).unwrap_err();

    assert!(error.contains("not managed by Skills Manage"));
    assert!(target_root.join("user-owned").join("SKILL.md").is_file());
}
```

- [ ] **Step 2: Run Rust tests and verify they fail**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test targets
```

Expected: tests fail because `targets.rs` does not exist and the command helper is undefined.

- [ ] **Step 3: Implement target management**

Implement these public pieces in `targets.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetToggleResult {
    pub target_id: String,
    pub target_name: String,
    pub skill_name: String,
    pub enabled: bool,
    pub changed: bool,
    pub target_path: String,
    pub message: String,
}

pub fn set_skill_target_enabled(source_path: String, target_id: String, enabled: bool) -> Result<TargetToggleResult, String>;

pub fn set_skill_target_enabled_with_root(
    source_path: &Path,
    target_id: &str,
    enabled: bool,
    data_root: &Path,
    target_root_override: &Path,
) -> Result<TargetToggleResult, String>;
```

Implementation details:

- Validate `source_path` is a directory with `SKILL.md`.
- Validate `source_path` canonical path starts inside `data_root/library`.
- Accept only target ids `codex` and `claude-code`.
- Use `target_root_override` in tests; production uses target id defaults from the user home directory.
- Copy recursively using the same exclusion rules as library import.
- Write `.skills-manage-link.json` after copy succeeds.
- Move managed target folders to `data_root/trash` during disable.

- [ ] **Step 4: Register the Tauri command**

Modify `lib.rs`:

```rust
mod targets;
use targets::{set_skill_target_enabled as set_skill_target_enabled_impl, TargetToggleResult};

#[tauri::command]
fn set_skill_target_enabled(
    source_path: String,
    target_id: String,
    enabled: bool,
) -> Result<TargetToggleResult, String> {
    set_skill_target_enabled_impl(source_path, target_id, enabled)
}
```

Add `set_skill_target_enabled` to `tauri::generate_handler!`.

- [ ] **Step 5: Run Rust tests and verify they pass**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test targets
cargo test
```

Expected: target tests and existing scanner/library tests pass.

## Task 2: Scanner Target State

**Files:**
- Modify: `apps/desktop/src-tauri/src/scanner.rs`
- Test: `apps/desktop/src-tauri/src/scanner.rs`

- [ ] **Step 1: Write failing scanner test**

Add a test proving a shared-library skill reports a managed target as enabled:

```rust
#[test]
fn shared_library_skill_reports_managed_target_enabled() {
    let data_root = unique_temp_dir("scan-target-state-data");
    let library_root = data_root.join("library");
    let codex_root = unique_temp_dir("scan-target-state-codex");
    let skill_dir = library_root.join("managed-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "---\nname: managed-skill\ndescription: Managed\n---\n").unwrap();
    fs::create_dir_all(codex_root.join("managed-skill")).unwrap();
    fs::write(codex_root.join("managed-skill").join("SKILL.md"), "# Managed copy\n").unwrap();
    fs::write(
        codex_root.join("managed-skill").join(".skills-manage-link.json"),
        format!(r#"{{"sourcePath":"{}","targetId":"codex"}}"#, skill_dir.display()),
    )
    .unwrap();

    let skills = scan_skill_roots_with_target_roots(
        &[library_root.clone()],
        &[("codex", codex_root.clone())],
    );

    let codex = skills[0].targets.iter().find(|target| target.id == "codex").unwrap();
    assert!(codex.enabled);
}
```

- [ ] **Step 2: Run scanner test and verify it fails**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test shared_library_skill_reports_managed_target_enabled
```

Expected: fails because scanner does not inspect managed target markers yet.

- [ ] **Step 3: Implement scanner target-state helper**

Add a testable scanner entrypoint:

```rust
pub fn scan_skill_roots_with_target_roots(
    roots: &[PathBuf],
    target_roots: &[(&str, PathBuf)],
) -> Vec<ScannedSkill>
```

Use it from `scan_skill_roots` with default target roots. For shared-library skills, mark a target enabled when `target_root/<skill-folder-name>/.skills-manage-link.json` exists and points at the same source path. For skills scanned from Codex or Claude Code roots, keep the current source-based enabled state.

- [ ] **Step 4: Run scanner and full Rust tests**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test shared_library_skill_reports_managed_target_enabled
cargo test
```

Expected: all Rust tests pass.

## Task 3: Frontend Target Toggle

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.css`
- Modify: `apps/desktop/src/App.test.tsx`

- [ ] **Step 1: Update frontend test**

Add a test where `scan_skills` first returns a shared-library skill with Codex disabled, `set_skill_target_enabled` returns enabled, and the second scan returns Codex enabled.

Expected assertions:

```ts
fireEvent.click(screen.getByRole("button", { name: "Enable Codex" }));
await waitFor(() =>
  expect(invokeMock).toHaveBeenCalledWith("set_skill_target_enabled", {
    sourcePath: sharedLibrarySkill.sourcePath,
    targetId: "codex",
    enabled: true,
  }),
);
await waitFor(() => expect(screen.getByText(/Enabled local-scan-skill for Codex/i)).toBeInTheDocument());
expect(screen.getByRole("button", { name: "Disable Codex" })).toBeInTheDocument();
```

- [ ] **Step 2: Run frontend test and verify it fails**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
```

Expected: fails because target buttons are still disabled and no command is wired.

- [ ] **Step 3: Use UI skills before changing UI**

Read `ui-ux-pro-max` and `design-taste-frontend`. Keep the target rows compact, preserve keyboard-accessible buttons, show status feedback below the action area, and keep VS Code disabled with clear visual disabled state.

- [ ] **Step 4: Implement target toggle UI**

Update `App.tsx` so:

- Target row buttons are enabled only for shared-library skills and supported target ids `codex` and `claude-code`.
- Button accessible names are `Enable Codex`, `Disable Codex`, `Enable Claude Code`, and `Disable Claude Code`.
- On click, call `set_skill_target_enabled` with `sourcePath`, `targetId`, and the next enabled state.
- Show toggling/result/error status.
- Re-run `scan_skills` after successful toggle.
- Keep VS Code disabled until a target path strategy exists.

- [ ] **Step 5: Run frontend tests**

Run:

```powershell
cd apps/desktop
npm test -- --run src/App.test.tsx
npm test -- --run
```

Expected: all frontend tests pass.

## Task 4: Verify and Publish

**Files:**
- All touched files.

- [ ] **Step 1: Run full verification**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
cd src-tauri
cargo fmt --check
cargo test
cargo check
```

- [ ] **Step 2: Confirm ignored local files**

Run:

```powershell
git status --short --ignored
```

Expected: local HTML files, `node_modules`, `dist`, `target`, `gen`, and `.codex-logs` remain ignored only.

- [ ] **Step 3: Commit, push, and open PR**

Run:

```powershell
git add apps/desktop docs/superpowers/plans/2026-06-04-target-enable-disable.md
git commit -m "feat: toggle skill targets"
git push -u origin codex/target-enable-disable
```

Open a draft PR with verification and filesystem safety notes.

## Self-Review

- Spec coverage: implements MVP item 6 for Codex and Claude Code target enable/disable, while leaving VS Code out until adapter rules are explicit.
- Placeholder scan: no unfinished marker text or undefined implementation steps.
- Type consistency: command names, result type names, target ids, and frontend invoke payloads match across tasks.
- Safety check: writes stay inside managed library, managed target copies, and managed trash; unmanaged target folders are never overwritten or removed.
