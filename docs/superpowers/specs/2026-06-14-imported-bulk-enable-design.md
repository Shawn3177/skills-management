# Imported tab — one-click bulk enable/disable per tool

## Context

The "Imported" tab lists shared-library skills with per-target toggle switches (Codex, Claude Code). To make a tool (e.g. Claude Code) use *all* imported skills, the user must flip each switch one by one — tedious with many skills. They want a one-click action per tool.

Decision (confirmed): **per-tool** bulk controls (not a single global "enable everything").

## Design

A "Bulk actions" bar at the top of the Imported panel (above the skill list, shown only when there is at least one imported skill). One row per toggleable tool:

```
Codex         [Enable all]  [Disable all]
Claude Code   [Enable all]  [Disable all]
```

VS Code stays "pending" and is excluded.

**Behavior** (`bulkSetTarget(targetId, targetName, enabled)`):
- Operate on the currently-visible `importedSkills` (so a search filter scopes the action to what's shown).
- Build the candidate set = imported skills whose target state differs from the desired state (already-correct skills are skipped).
- Loop, calling the existing `set_skill_target_enabled` backend command per candidate. Per-item `try/catch` so one failure does not abort the rest (same resilience pattern as `importAllSkills`).
- Progress reuses the existing `actions.enablingSkillForTarget` / `disablingSkillForTarget` messages; on completion show a summary `Enabled N skills for {tool}.` / `Disabled N skills for {tool}.`
- Reload once at the end (`loadSkills({ keepDetailOpen: detailOpen, preserveImportMessage: true })`).
- Reuses `targetActionState` / `targetActionMessage` (shown via the panel's `StatusMessage`) and locks all controls while running (`targetToggleLocked`).

**Button disabled logic** (per tool, computed over visible imported skills):
- `Enable all` disabled when locked, no imported skills, or all already enabled.
- `Disable all` disabled when locked or none enabled.

**Accessibility:** visible label is generic ("Enable all" / "Disable all"); `aria-label` is tool-qualified ("Enable all for Claude Code").

## Reuse / files

- Backend `set_skill_target_enabled` unchanged.
- `apps/desktop/src/App.tsx` — add `bulkSetTarget`, render the bulk bar in the Imported panel.
- `apps/desktop/src/App.css` — `.bulk-target-bar` / `.bulk-target-row` / `.bulk-target-actions`.
- `apps/desktop/src/i18n/messages.ts` — new keys (both locales): `actions.enableAll`, `actions.disableAll`, `actions.enableAllForTarget`, `actions.disableAllForTarget`, `actions.enabledAllForTarget`, `actions.disabledAllForTarget`, `actions.bulkTargetNoChange`, `workspace.import.bulkActions`.
- `apps/desktop/src/App.test.tsx` — bulk-enable calls + summary test; disabled-state test.

## Out of scope (YAGNI)

No global "all tools at once" button, no detail-view changes, no confirm dialog (disable is a recoverable soft-delete to trash).

## Verification

`npm test -- --run` and `npm run build` in `apps/desktop`; manual check in the Imported tab (browser preview is fine — demo data includes shared-library skills).
