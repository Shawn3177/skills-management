# Skills Manage Bilingual CC Switch UI Design

## Decision

Skills Manage will support Chinese and English interface text, with Chinese as the default language for the current user workflow. The frontend will move toward a CC Switch-like local control surface: light, compact, list-first, and optimized for switching skills on or off across local tools.

This design is limited to the renderer UI and lightweight preference handling. It does not change backend filesystem write behavior.

## Goals

- Show all application chrome, action labels, status text, empty states, and error messages in Chinese or English.
- Keep skill names, skill descriptions, support file names, source paths, and target paths as source data rather than translating them.
- Add a visible language switch that can change the UI immediately.
- Persist the selected language locally so the next launch keeps the same language.
- Refresh the current app surface to feel closer to CC Switch: compact top controls, list-first management, direct target switches, shallow surfaces, and restrained styling.
- Preserve accessibility: labelled controls, visible focus states, keyboard operation, readable contrast, and clear disabled states.

## Non-Goals

- No cloud-synced language preference in this phase.
- No machine translation for skill content.
- No full routing system or settings screen implementation unless needed to host the language control.
- No provider switching, proxy, usage dashboard, session search, or other CC Switch features outside skills management.
- No backend rewrite for SQLite or `settings.json` persistence in this UI-only phase.

## Product Direction

The reference direction is CC Switch's desktop control surface. The useful traits for this project are:

- A light desktop app surface with compact spacing and low visual noise.
- A top mode switch for supported tools or modules.
- Large, scan-friendly rows where each item has a primary name, secondary path or detail, status, and direct actions.
- Dialog and form patterns built from presets, chips, clear labels, and a single primary action.
- A practical operations feel rather than a marketing dashboard.

Skills Manage should adapt that pattern to skills:

- Top area: product title, primary module tabs, language switch, refresh/import controls.
- Main list: discovered or managed skills, health state, source, target enable count, and quick selection.
- Detail area: selected skill metadata, target tool switches, import/export actions, and status messages.
- Footer/status strip: local data root, backup mode, package format, and current safe-write posture.

## Language Model

The frontend will define a small typed locale layer:

- `Locale`: `zh-CN` or `en-US`.
- `defaultLocale`: `zh-CN`.
- `messages`: a nested dictionary keyed by stable semantic message ids.
- `t(key, params?)`: resolves a localized message and interpolates simple values.
- `useLocale`: local state plus persistence in `localStorage`.

Initial persistence uses `localStorage` because this phase is renderer-only and low risk. A later settings phase can mirror the same preference into `%USERPROFILE%\.skills-manage\settings.json` through Rust.

Message ids should describe product meaning rather than current English copy. Examples:

- `app.title`
- `nav.skills`
- `status.scan.scanning`
- `status.scan.ready`
- `status.scan.fallback`
- `actions.importToLibrary`
- `targets.enabled`
- `targets.disabled`
- `errors.scanUnavailable`

Dynamic messages should use named parameters:

- `actions.enablingSkillForTarget`: `正在为 {targetName} 启用 {skillName}`
- `actions.enabledSkillForTarget`: `已为 {targetName} 启用 {skillName}`

## UI Structure

### App Shell

The shell remains a desktop-first grid, but the visual hierarchy will shift from the current dark rail plus large detail area to a lighter CC Switch-inspired control layout:

- A compact title row with app name, current module tabs, utility buttons, and language switch.
- A left or central skills list that remains the primary scanning surface.
- A right detail panel for the selected skill and target switches.
- A bottom status strip with local safety information.

The layout can keep the current React component boundaries, but the visual treatment should reduce heavy shadows, reduce tall card spacing, and make rows feel more like operational list entries.

### Navigation And Module Tabs

The current sections remain visible:

- Skills
- Import
- Packages
- Settings

Only `Skills` is fully active in this phase. Inactive sections can remain disabled or visually secondary, but they must have bilingual labels and accessible names.

### Language Switch

The language switch should sit in the top utility area and use two clear options:

- `中文`
- `EN`

It should behave like a segmented control, not a hidden settings-only preference. The selected language should be visually obvious and announced through button state.

### Skills List

Rows should be compact and information-dense:

- Health icon and health label.
- Skill name.
- Description or fallback text.
- Source.
- Enabled target count.
- Selected row state.

The row should not resize unpredictably when switching languages. Longer Chinese or English strings should truncate or wrap only in planned secondary text areas.

### Skill Detail

The detail panel should show:

- Selected skill title and health badge.
- Description.
- Metadata table for source, path, and support files.
- Target tool switches for Codex and Claude Code when the skill is in the shared library.
- Disabled target controls with explanatory text when the skill is not managed yet.
- Actions for import, repair, and export.

The target tool controls should feel like switches or compact action rows, closer to a control panel than plain text buttons.

## Component Boundaries

The first implementation can split the current `App.tsx` without over-abstracting:

- `i18n/messages.ts`: locale dictionaries and types.
- `i18n/useLocale.ts`: locale state, persistence, and translation helper.
- `components/LanguageSwitch.tsx`: segmented language control.
- `components/AppShell.tsx`: top bar, layout, footer.
- `components/SkillsList.tsx`: search, status, rows, empty state.
- `components/SkillDetail.tsx`: metadata, targets, actions, operation messages.

If keeping fewer files makes the first patch safer, `LanguageSwitch` and i18n should still be separate so tests can cover them without rendering the whole app.

## Data Flow

1. App starts with `zh-CN` unless `localStorage` contains a valid locale.
2. `useLocale` exposes `locale`, `setLocale`, and `t`.
3. UI components receive `t` or localized labels from the top-level app.
4. Scan/import/target toggles keep their current Tauri command calls.
5. Success and error messages use localized templates.
6. Skill records remain unchanged and are displayed as scanned.
7. Language switching does not re-run scans, imports, or target writes.

## Error Handling

- If stored locale is unknown or malformed, fall back to `zh-CN`.
- If a message key is missing in the selected language, fall back to `en-US`.
- If both language entries are missing, show the key in development-friendly form rather than crashing.
- Existing backend errors should be shown as-is when they are raw filesystem or Rust errors, but surrounding recovery text should be localized.
- Disabled actions should explain why they are disabled through visible text or accessible labels when practical.

## Visual Design Rules

- Use the existing lucide icon family consistently.
- Do not use emoji as interface icons.
- Use a light neutral base with one practical blue accent and semantic success/warning/danger colors.
- Avoid decorative gradients, oversized hero treatment, nested cards, and marketing-page spacing.
- Use 8px or smaller card radius unless a pill shape is intentionally used for segmented controls or badges.
- Keep primary controls at least 44px tall.
- Keep focus rings visible.
- Preserve responsive behavior for narrow windows, even though Windows desktop is the primary target.

## Accessibility

- Every icon-only button needs an `aria-label`.
- The language switch needs selected state through `aria-pressed` or equivalent semantics.
- Scan, import, and target operation messages need `role="status"` or `role="alert"` as appropriate.
- Health status must use icon plus text, not color alone.
- Target enabled state must be readable in both languages.
- Keyboard focus order should follow top controls, search/list, detail actions, footer.

## Testing Strategy

Frontend tests should cover:

- Default locale is `zh-CN`.
- A valid stored locale is restored.
- An invalid stored locale falls back to `zh-CN`.
- The language switch updates visible labels.
- Search placeholder and scan status render in both languages.
- Import and target operation messages use localized templates.
- Skill data remains unchanged by locale switching.

Build checks:

- `npm test -- --run`
- `npm run build`

Manual visual verification:

- Open the local app at the Vite/Tauri development target.
- Capture one screenshot in Chinese and one in English.
- Verify no text overlaps in compact rows, target controls, action bar, and status strip.

## Implementation Order

1. Add locale dictionaries and tests for fallback behavior.
2. Add `useLocale` with `localStorage` persistence.
3. Add `LanguageSwitch` and wire it into the top utility area.
4. Replace hard-coded UI chrome strings with message ids.
5. Refresh CSS toward the CC Switch-inspired compact light control surface.
6. Verify unit tests, build, and browser screenshots.

## Risks

- Some English labels are shorter than Chinese labels and can hide layout issues until both languages are tested.
- Raw backend errors may remain English until Rust command errors are normalized.
- Persisting in `localStorage` is acceptable for this UI phase, but it should later move into `settings.json` for consistency with the product architecture.
- A visual refresh can accidentally mix current dark-rail styling with the new light control-surface direction; the implementation should commit to one theme.

## References

- CC Switch homepage: https://ccswitch.ai/
- CC Switch main interface image: https://ccswitch.ai/cc-switch-main.png
- CC Switch add provider image: https://ccswitch.ai/cc-switch-add.png
- Existing v1 technical design: `docs/superpowers/specs/2026-06-03-skills-manage-v1-technical-design.md`
- Project rules: `AGENTS.md`
