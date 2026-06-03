# Repository Workflow Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add repository workflow assets so Skills Manage has clear Codex instructions, pull request hygiene, CI checks, and a documented branch-based development rhythm.

**Architecture:** This is a repository-process change only. It creates root-level agent instructions, GitHub templates, a defensive CI workflow, and a short development guide without changing product code.

**Tech Stack:** Markdown, GitHub Actions, Git.

---

### Task 1: Agent Instructions

**Files:**
- Create: `AGENTS.md`

- [ ] **Step 1: Create `AGENTS.md`**

Write a project-specific instruction file that covers the product goal, stack, repository layout, local HTML handling, development workflow, test commands, frontend design skills, and safety rules.

- [ ] **Step 2: Verify the file**

Run: `Get-Content AGENTS.md -Encoding utf8`

Expected: The file lists Tauri 2, React, TypeScript, Rust, SQLite, `ui-ux-pro-max`, and `design-taste-frontend`.

### Task 2: Pull Request Template

**Files:**
- Create: `.github/pull_request_template.md`

- [ ] **Step 1: Create the PR template**

Write a short template with summary, scope, verification, screenshots, risks, and checklist sections.

- [ ] **Step 2: Verify the file**

Run: `Get-Content .github/pull_request_template.md -Encoding utf8`

Expected: The template asks for verification commands and confirms local HTML files are not included.

### Task 3: Baseline CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create a defensive CI workflow**

The workflow should run on pushes and pull requests. It should check out the repo, verify required docs exist, ensure ignored local HTML references are not tracked, and run Node/Rust checks only when project files exist.

- [ ] **Step 2: Verify workflow syntax by inspection**

Run: `Get-Content .github/workflows/ci.yml -Encoding utf8`

Expected: The workflow has a `repository-hygiene` job and conditional frontend/Rust checks.

### Task 4: Development Workflow Guide

**Files:**
- Create: `docs/development-workflow.md`

- [ ] **Step 1: Create the guide**

Document the issue to branch to PR workflow, how to use Codex, when to update `AGENTS.md`, how to use UI design skills before frontend work, and what should never be committed.

- [ ] **Step 2: Verify the guide**

Run: `Get-Content docs/development-workflow.md -Encoding utf8`

Expected: The guide includes branch naming, PR review, CI, and local HTML handling.

### Task 5: Final Verification

**Files:**
- Read: all files above

- [ ] **Step 1: Check repository status**

Run: `git status --short --branch --ignored`

Expected: New workflow files are tracked or ready to be tracked; `agent_project_process_reference.html` and `preview.html` remain ignored.

- [ ] **Step 2: Check for unfinished marker text**

Run: `rg -n "TODO:|TBD:" AGENTS.md .github docs -g "!docs/superpowers/plans/**" -g "!.github/workflows/ci.yml"`

Expected: No unfinished marker text.

- [ ] **Step 3: Commit**

Run:

```bash
git add AGENTS.md .github docs
git commit -m "chore: add repository workflow assets"
```

Expected: Commit succeeds on a feature branch.
