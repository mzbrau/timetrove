/** Build a Jira Cloud browse URL from site settings + issue key. */
export function jiraBrowseUrl(siteUrl: string, issueKey: string): string | null {
  const base = siteUrl.trim().replace(/\/+$/, "");
  const key = issueKey.trim().toUpperCase();
  if (!base.startsWith("https://") || !key) return null;
  return `${base}/browse/${key}`;
}

/** Percent of estimate used; null when estimate missing. Prefer TimeTrove when linked. */
export function jiraPercentComplete(
  estimateSeconds: number | null | undefined,
  timetroveBookedMs: number | null | undefined,
  jiraSpentSeconds: number | null | undefined,
): number | null {
  if (estimateSeconds == null || estimateSeconds <= 0) return null;
  const estimateMs = estimateSeconds * 1000;
  if (timetroveBookedMs != null && timetroveBookedMs >= 0) {
    return (timetroveBookedMs / estimateMs) * 100;
  }
  if (jiraSpentSeconds != null && jiraSpentSeconds >= 0) {
    return (jiraSpentSeconds * 1000 / estimateMs) * 100;
  }
  return null;
}
