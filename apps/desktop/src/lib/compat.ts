import type { TFunction } from "../i18n/useLocale";
import type { MessageKey } from "../i18n/messages";
import type { CompatFinding, CompatSummary, SkillRecord, ToolId } from "./skills";

/** Rules the backend reports (see `compat.rs`), each with its own wording. */
const KNOWN_RULES = [
  "cc-hooks",
  "cc-fork",
  "cc-name",
  "cc-frontmatter",
  "cc-tools",
  "cc-args",
  "cc-paths",
  "cx-tools",
  "cx-paths",
  "wrapper",
  "declared",
];

/** The compat entry for a tool, when the skill may not suit it. */
export function compatFor(record: SkillRecord, toolId: ToolId): CompatSummary | null {
  return record.compat?.find((entry) => entry.targetId === toolId && entry.level !== "compatible") ?? null;
}

/** A compat entry the user has not confirmed: enabling there warns first. */
export function compatWarning(record: SkillRecord, toolId: ToolId): CompatSummary | null {
  const entry = compatFor(record, toolId);
  return entry && !entry.acknowledged ? entry : null;
}

/** Why a finding matters, in words, e.g. "uses the Claude Code tool TodoWrite". */
export function compatReason(t: TFunction, finding: CompatFinding): string {
  const key = KNOWN_RULES.includes(finding.ruleId) ? (`compat.rule.${finding.ruleId}` as MessageKey) : "compat.rule.unknown";
  return t(key, { token: finding.token });
}

/** Where a finding is, e.g. "SKILL.md line 42". */
export function compatPlace(t: TFunction, finding: CompatFinding): string {
  return finding.line ? t("compat.where", { file: finding.file, line: finding.line }) : finding.file;
}

/** The enable-time warning for one tool. */
export function compatAsk(t: TFunction, skill: SkillRecord, toolName: string, entry: CompatSummary): string {
  return t(entry.level === "incompatible" ? "compat.ask" : "compat.askDegraded", {
    skill: skill.name,
    tool: toolName,
    reason: compatReason(t, entry.first),
    where: compatPlace(t, entry.first),
    more: entry.count > 1 ? t("compat.more", { count: entry.count - 1 }) : "",
  });
}
