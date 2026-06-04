# Tauri Desktop Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the first runnable Tauri 2 desktop shell for Skills Manage, with a compact CC Switch-like local control surface and basic verification.

**Architecture:** The app lives in `apps/desktop`. React owns the renderer UI and uses typed local model helpers for the initial shell. Rust owns Tauri commands and will later own filesystem/database work; this phase adds only safe read-only status commands and tray-ready app metadata.

**Tech Stack:** Tauri 2, React, TypeScript, Vite, Tailwind CSS, Vitest, Rust, Cargo.

---

## File Structure

- Create `apps/desktop/`: Tauri desktop app root.
- Create `apps/desktop/src/`: React renderer.
- Create `apps/desktop/src/lib/skills.ts`: pure frontend helpers for filtering and status counts.
- Create `apps/desktop/src/lib/skills.test.ts`: Vitest coverage for helper behavior.
- Create `apps/desktop/src/data/demoSkills.ts`: local demo records for the first shell before scanner integration.
- Create `apps/desktop/src/App.tsx`: app shell, skill list, detail pane, import/export/settings surfaces.
- Create `apps/desktop/src/App.css`: product UI styling with compact desktop layout.
- Create `apps/desktop/src-tauri/`: Rust Tauri backend.
- Modify `.github/workflows/ci.yml`: run checks against `apps/desktop` once it exists.
- Modify `README.md`: document local run commands.

## Design Decisions

- Use a dense three-area product layout: narrow navigation rail, skill list, detail/work panel.
- Avoid marketing hero composition. The first screen is the usable app surface.
- Use restrained neutral colors with one teal/blue accent for active status and focus states.
- Keep filesystem-changing actions visibly disabled or simulated until Rust safety modules exist.
- Keep local HTML references ignored and out of the repo. They only inform the workflow and UI density.

## Task 1: Scaffold the Tauri App

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: generated Vite/Tauri app files under `apps/desktop/`

- [ ] **Step 1: Configure package downloads**

Run npm package installs through a China-friendly registry. TUNA has official Node.js binary mirrors, but not a clearly documented official npm registry page, so use a one-command registry override for npm package operations instead of permanently changing the user's global config.

Run:

```powershell
npm config get registry
```

- [ ] **Step 2: Generate the Tauri React TypeScript template**

Run:

```powershell
npm create tauri-app@latest apps/desktop -- --template react-ts --manager npm --identifier com.shawn.skillsmanage --tauri-version 2 --yes
```

Expected: the React TypeScript Tauri 2 template is created without interactive prompts.

- [ ] **Step 3: Install dependencies**

Run:

```powershell
cd apps/desktop
npm install --registry=https://registry.npmmirror.com
npm install -D vitest @testing-library/react @testing-library/jest-dom jsdom tailwindcss @tailwindcss/vite --registry=https://registry.npmmirror.com
npm install lucide-react --registry=https://registry.npmmirror.com
```

Expected: dependencies are installed and `package-lock.json` is created.

## Task 2: Frontend Helper TDD

**Files:**
- Create: `apps/desktop/src/lib/skills.test.ts`
- Create: `apps/desktop/src/lib/skills.ts`
- Create: `apps/desktop/src/data/demoSkills.ts`

- [ ] **Step 1: Write failing tests**

Create `apps/desktop/src/lib/skills.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { filterSkills, getSkillStats, type SkillRecord } from "./skills";

const records: SkillRecord[] = [
  {
    id: "codex-style",
    name: "codex-style",
    description: "Codex UI and workflow guardrails",
    source: "Codex",
    sourcePath: "C:/Users/example/.codex/skills/codex-style",
    health: "healthy",
    targets: [
      { id: "codex", name: "Codex", enabled: true },
      { id: "claude", name: "Claude Code", enabled: false },
    ],
    supportFiles: ["SKILL.md", "references/design.md"],
  },
  {
    id: "agent-pack",
    name: "agent-pack",
    description: "Reusable agent tool design rules",
    source: "Shared Library",
    sourcePath: "C:/Users/example/.skills-manage/library/agent-pack",
    health: "warning",
    targets: [{ id: "codex", name: "Codex", enabled: false }],
    supportFiles: ["SKILL.md"],
  },
];

describe("filterSkills", () => {
  it("matches name, description, source, and path without case sensitivity", () => {
    expect(filterSkills(records, "WORKFLOW")).toHaveLength(1);
    expect(filterSkills(records, "shared")).toHaveLength(1);
    expect(filterSkills(records, ".codex")).toHaveLength(1);
  });

  it("returns all records for a blank query", () => {
    expect(filterSkills(records, "   ")).toEqual(records);
  });
});

describe("getSkillStats", () => {
  it("counts healthy skills, warnings, and enabled target links", () => {
    expect(getSkillStats(records)).toEqual({
      total: 2,
      healthy: 1,
      warnings: 1,
      enabledTargets: 1,
    });
  });
});
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cd apps/desktop
npm test -- --run src/lib/skills.test.ts
```

Expected: fails because `src/lib/skills.ts` does not exist yet.

- [ ] **Step 3: Implement helpers**

Create `apps/desktop/src/lib/skills.ts`:

```ts
export type SkillHealth = "healthy" | "warning" | "broken";

export interface TargetState {
  id: string;
  name: string;
  enabled: boolean;
}

export interface SkillRecord {
  id: string;
  name: string;
  description: string;
  source: string;
  sourcePath: string;
  health: SkillHealth;
  targets: TargetState[];
  supportFiles: string[];
}

export interface SkillStats {
  total: number;
  healthy: number;
  warnings: number;
  enabledTargets: number;
}

export function filterSkills(records: SkillRecord[], query: string): SkillRecord[] {
  const normalized = query.trim().toLowerCase();

  if (!normalized) {
    return records;
  }

  return records.filter((record) => {
    const haystack = [
      record.name,
      record.description,
      record.source,
      record.sourcePath,
      record.supportFiles.join(" "),
    ]
      .join(" ")
      .toLowerCase();

    return haystack.includes(normalized);
  });
}

export function getSkillStats(records: SkillRecord[]): SkillStats {
  return {
    total: records.length,
    healthy: records.filter((record) => record.health === "healthy").length,
    warnings: records.filter((record) => record.health === "warning").length,
    enabledTargets: records.reduce(
      (count, record) => count + record.targets.filter((target) => target.enabled).length,
      0,
    ),
  };
}
```

- [ ] **Step 4: Add demo data**

Create `apps/desktop/src/data/demoSkills.ts` with realistic Codex, Claude Code, and shared library examples that include healthy, warning, and broken states.

- [ ] **Step 5: Run tests and verify they pass**

Run:

```powershell
cd apps/desktop
npm test -- --run src/lib/skills.test.ts
```

Expected: test file passes.

## Task 3: Build the First App Surface

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/App.css`
- Modify: `apps/desktop/src/main.tsx`

- [ ] **Step 1: Replace starter UI**

Implement:

- left rail with sections: Skills, Import, Packages, Settings.
- toolbar with search input and scan button.
- skill list with health state, source, enabled target count.
- detail panel with metadata, target toggles, support files, and safe action buttons.
- lower status strip showing data root, backup mode, and package format.

- [ ] **Step 2: Keep unsafe actions inert**

Buttons that imply filesystem changes must be visible but disabled or clearly marked as not wired yet:

- Import folder.
- Enable target.
- Disable target.
- Export `.skillpack`.
- Repair broken link.

- [ ] **Step 3: Style for desktop density**

Use CSS variables and a compact layout. Preserve:

- 44px minimum hit targets for important controls.
- visible focus states.
- readable contrast.
- no nested card containers.
- no decorative gradient/orb background.

## Task 4: Add a Safe Rust Status Command

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Test: Rust unit tests in the same file or a focused module.

- [ ] **Step 1: Write a Rust test for app status**

Add a test that expects `default_app_status()` to return:

- app name: `Skills Manage`
- data root suffix: `.skills-manage`
- safe write mode: `preview`

- [ ] **Step 2: Run the Rust test and verify it fails**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: fails because `default_app_status` is not implemented.

- [ ] **Step 3: Implement the status command**

Add a serializable status struct and expose a Tauri command named `get_app_status`.

- [ ] **Step 4: Run the Rust test and verify it passes**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
```

Expected: tests pass.

## Task 5: Update CI and Documentation

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`

- [ ] **Step 1: Point CI at the desktop app**

Update workflow checks so frontend install/build/test runs from `apps/desktop`, and Rust check/test runs from `apps/desktop/src-tauri`.

- [ ] **Step 2: Document local commands**

Update `README.md` with:

```powershell
cd apps/desktop
npm install
npm run dev
npm run tauri dev
npm run build
npm run tauri build -- --no-bundle
```

Also document that local reference HTML files remain ignored.

## Task 6: Verify and Publish Branch

**Files:**
- All touched files.

- [ ] **Step 1: Run frontend checks**

Run:

```powershell
cd apps/desktop
npm test -- --run
npm run build
```

- [ ] **Step 2: Run Rust checks**

Run:

```powershell
cd apps/desktop/src-tauri
cargo test
cargo check
```

- [ ] **Step 3: Confirm ignored HTML files are not staged**

Run:

```powershell
git status --short --ignored
```

Expected: `agent_project_process_reference.html` and `preview.html` appear only as ignored files.

- [ ] **Step 4: Commit and push**

Run:

```powershell
git add apps/desktop README.md .github/workflows/ci.yml docs/superpowers/plans/2026-06-04-tauri-desktop-shell.md
git commit -m "feat: add Tauri desktop shell"
git push -u origin codex/tauri-desktop-shell
```

## Self-Review

- Spec coverage: this plan implements the first MVP item, prepares UI surfaces for scan/import/export/settings, and keeps filesystem writes out of the frontend.
- Placeholder scan: no `TODO`, `TBD`, or unbounded "implement later" steps are used.
- Type consistency: `SkillRecord`, `TargetState`, and `SkillStats` are defined once in `src/lib/skills.ts` and reused by tests, demo data, and UI.
