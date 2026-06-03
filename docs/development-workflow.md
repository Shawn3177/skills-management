# Development Workflow

This project follows a lightweight GitHub flow: issue, branch, focused change, verification, pull request, review, merge.

## Branches

Use `main` as the stable branch. Do not do long-running feature work directly on `main`.

Recommended branch names:

- `feat/tauri-shell`
- `feat/skill-scanner`
- `fix/windows-path-handling`
- `chore/repo-workflow-assets`
- `docs/technical-plan`

## Issue To Branch To PR

1. Define the work in one sentence.
2. Add acceptance criteria.
3. Create a focused branch.
4. Ask Codex to read the relevant docs first.
5. For complex work, ask Codex to write a plan before coding.
6. Implement the smallest useful slice.
7. Run verification commands.
8. Review the diff.
9. Open a pull request.
10. Wait for CI and review before merging.

## How To Use Codex

Good first prompt for a feature:

```text
Read AGENTS.md and docs/skills-manage-v1-plan.md first.
Then inspect the current app structure.
Propose a focused plan for the first useful slice of <feature>.
Do not implement until the plan is clear.
```

Good implementation prompt:

```text
Use the approved plan.
Keep the change scoped.
Run the relevant verification command.
Report files changed, verification result, and any remaining risk.
```

Good review prompt:

```text
Review this branch for correctness, filesystem safety, UI consistency, test coverage, and unrelated changes.
Lead with concrete findings and file references.
```

## Frontend Work

Before building frontend screens, use these local design skills:

- `ui-ux-pro-max`
- `design-taste-frontend`

For product UI brief clarification, use Product Design `get-context`.

The product is a desktop operations tool. Prefer:

- predictable navigation
- compact but readable lists
- clear empty, loading, and error states
- accessible focus states
- one icon family
- restrained colors
- no decorative emoji icons

## CI

The initial CI verifies repository hygiene and skips frontend or Rust checks until the Tauri app exists.

After `apps/desktop` is created, CI should run:

```text
npm ci
npm run build --if-present
cargo check
cargo test
```

If the app later needs Tauri bundle checks, add a separate packaging workflow because desktop packaging has heavier OS-specific prerequisites.

## Files That Must Stay Local

Do not commit these files:

- `agent_project_process_reference.html`
- `preview.html`

They are local reference documents and are intentionally ignored.

## Update AGENTS.md When Rules Change

Update `AGENTS.md` when any of these change:

- project structure
- setup commands
- test commands
- frontend design rules
- safety rules
- packaging process
- GitHub workflow expectations
