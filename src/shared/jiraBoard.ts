import { jiraPercentComplete } from "./jira";
import type { JiraDashboardIssue } from "./types";

export type JiraViewMode = "list" | "board";

export type JiraSortKey =
  | "key"
  | "summary"
  | "status"
  | "priority"
  | "estimate"
  | "timetrove"
  | "jiraSpent"
  | "pct"
  | "task";

export type JiraSortDir = "asc" | "desc";

export function issueStatusLabel(issue: JiraDashboardIssue): string {
  const status = issue.status.trim();
  return status || "Unknown";
}

export function filterJiraIssues(
  issues: JiraDashboardIssue[],
  query: string,
): JiraDashboardIssue[] {
  const q = query.trim().toLowerCase();
  if (!q) return issues;
  return issues.filter((issue) => {
    const key = issue.issueKey.toLowerCase();
    const summary = issue.summary.toLowerCase();
    return key.includes(q) || summary.includes(q);
  });
}

function cmpNullableNumber(
  a: number | null | undefined,
  b: number | null | undefined,
): number {
  const aMissing = a == null || !Number.isFinite(a);
  const bMissing = b == null || !Number.isFinite(b);
  if (aMissing && bMissing) return 0;
  if (aMissing) return 1;
  if (bMissing) return -1;
  return (a as number) - (b as number);
}

function cmpText(a: string, b: string): number {
  return a.localeCompare(b, undefined, { sensitivity: "base" });
}

function sortValue(
  issue: JiraDashboardIssue,
  key: JiraSortKey,
): string | number | null {
  switch (key) {
    case "key":
      return issue.issueKey;
    case "summary":
      return issue.summary;
    case "status":
      return issueStatusLabel(issue);
    case "priority":
      return issue.priority;
    case "estimate":
      return issue.estimateSeconds;
    case "timetrove":
      return issue.timetroveBookedMs;
    case "jiraSpent":
      return issue.totalSpentSeconds;
    case "pct":
      return jiraPercentComplete(
        issue.estimateSeconds,
        issue.timetroveBookedMs,
        issue.totalSpentSeconds,
      );
    case "task":
      return issue.taskName ?? "";
  }
}

export function sortJiraIssues(
  issues: JiraDashboardIssue[],
  key: JiraSortKey,
  dir: JiraSortDir,
): JiraDashboardIssue[] {
  const mult = dir === "asc" ? 1 : -1;
  return [...issues].sort((a, b) => {
    if (key === "priority") {
      const byRank = cmpNullableNumber(a.priorityRank, b.priorityRank);
      if (byRank !== 0) return byRank * mult;
      const byName = cmpText(a.priority || "", b.priority || "");
      if (byName !== 0) return byName * mult;
      return cmpText(a.issueKey, b.issueKey) * mult;
    }

    const av = sortValue(a, key);
    const bv = sortValue(b, key);
    let cmp: number;
    if (typeof av === "number" || typeof bv === "number" || av == null || bv == null) {
      cmp = cmpNullableNumber(
        typeof av === "number" ? av : null,
        typeof bv === "number" ? bv : null,
      );
    } else {
      cmp = cmpText(String(av), String(bv));
    }
    if (cmp !== 0) return cmp * mult;
    return cmpText(a.issueKey, b.issueKey) * mult;
  });
}

/** Merge saved lane order with statuses present in the filtered issue list. */
export function resolveBoardLanes(
  issues: JiraDashboardIssue[],
  savedOrder: string[],
): string[] {
  const present = new Set(issues.map(issueStatusLabel));
  const ordered: string[] = [];
  for (const name of savedOrder) {
    if (present.has(name) && !ordered.includes(name)) ordered.push(name);
  }
  const extras = [...present]
    .filter((name) => !ordered.includes(name))
    .sort((a, b) => cmpText(a, b));
  return [...ordered, ...extras];
}

/** Reorder lanes after dragging `from` onto `to` (both indices in current lane list). */
export function reorderLanes(
  lanes: string[],
  fromIndex: number,
  toIndex: number,
): string[] {
  if (
    fromIndex < 0 ||
    toIndex < 0 ||
    fromIndex >= lanes.length ||
    toIndex >= lanes.length ||
    fromIndex === toIndex
  ) {
    return lanes;
  }
  const next = [...lanes];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

/**
 * Persist order: keep relative order of statuses still present, then append
 * any previously saved statuses that are temporarily empty so they reappear later.
 */
export function mergeLaneOrderForPersist(
  visibleOrdered: string[],
  previousSaved: string[],
): string[] {
  const seen = new Set(visibleOrdered);
  const next = [...visibleOrdered];
  for (const name of previousSaved) {
    if (!seen.has(name)) {
      next.push(name);
      seen.add(name);
    }
  }
  return next;
}
