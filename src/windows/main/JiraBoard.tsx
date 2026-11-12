import { useState } from "react";
import { formatHm } from "../../shared/format";
import { jiraPercentComplete } from "../../shared/jira";
import {
  issueStatusLabel,
  mergeLaneOrderForPersist,
  reorderLanes,
} from "../../shared/jiraBoard";
import type { JiraDashboardIssue } from "../../shared/types";

type Props = {
  issues: JiraDashboardIssue[];
  lanes: string[];
  savedLaneOrder: string[];
  onLaneOrderChange: (nextOrder: string[]) => void;
  onOpenIssue: (issue: JiraDashboardIssue) => void;
};

function fmtPct(n: number | null): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return `${Math.round(n)}%`;
}

export function JiraBoard({
  issues,
  lanes,
  savedLaneOrder,
  onLaneOrderChange,
  onOpenIssue,
}: Props) {
  const [dragFrom, setDragFrom] = useState<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);

  const byStatus = new Map<string, JiraDashboardIssue[]>();
  for (const issue of issues) {
    const status = issueStatusLabel(issue);
    const list = byStatus.get(status);
    if (list) list.push(issue);
    else byStatus.set(status, [issue]);
  }

  const commitReorder = (fromIndex: number, toIndex: number) => {
    const reordered = reorderLanes(lanes, fromIndex, toIndex);
    if (reordered === lanes) return;
    onLaneOrderChange(mergeLaneOrderForPersist(reordered, savedLaneOrder));
  };

  return (
    <div className="jira-board" role="list">
      {lanes.map((lane, index) => {
        const cards = byStatus.get(lane) ?? [];
        const isOver = dragOver === index && dragFrom !== null && dragFrom !== index;
        return (
          <section
            key={lane}
            className={`jira-lane${isOver ? " jira-lane-drop" : ""}`}
            role="listitem"
          >
            <header
              className="jira-lane-header"
              draggable
              title="Drag to reorder lanes"
              onDragStart={(e) => {
                setDragFrom(index);
                e.dataTransfer.effectAllowed = "move";
                e.dataTransfer.setData("text/plain", String(index));
              }}
              onDragEnd={() => {
                setDragFrom(null);
                setDragOver(null);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                setDragOver(index);
              }}
              onDragLeave={() => {
                setDragOver((cur) => (cur === index ? null : cur));
              }}
              onDrop={(e) => {
                e.preventDefault();
                const raw =
                  e.dataTransfer.getData("text/plain") ||
                  (dragFrom != null ? String(dragFrom) : "");
                const from = Number.parseInt(raw, 10);
                setDragFrom(null);
                setDragOver(null);
                if (Number.isFinite(from)) commitReorder(from, index);
              }}
            >
              <span className="jira-lane-title">{lane}</span>
              <span className="jira-lane-count mono muted">{cards.length}</span>
            </header>
            <div className="jira-lane-cards">
              {cards.map((issue) => {
                const pct = jiraPercentComplete(
                  issue.estimateSeconds,
                  issue.timetroveBookedMs,
                  issue.totalSpentSeconds,
                );
                return (
                  <article
                    key={issue.issueKey}
                    className="jira-card"
                    onClick={() => onOpenIssue(issue)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        onOpenIssue(issue);
                      }
                    }}
                    role="button"
                    tabIndex={0}
                  >
                    <div className="jira-card-top">
                      <span className="jira-card-key mono">{issue.issueKey}</span>
                      {issue.priority ? (
                        <span className="jira-card-priority">{issue.priority}</span>
                      ) : null}
                    </div>
                    <p className="jira-card-summary">
                      {issue.summary || "—"}
                    </p>
                    <div className="jira-card-meta muted mono">
                      {issue.timetroveBookedMs != null
                        ? formatHm(issue.timetroveBookedMs)
                        : "—"}
                      <span aria-hidden="true"> · </span>
                      {fmtPct(pct)}
                    </div>
                  </article>
                );
              })}
              {cards.length === 0 && (
                <p className="jira-lane-empty muted">No issues</p>
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}
