import { describe, expect, it } from "vitest";
import {
  filterJiraIssues,
  mergeLaneOrderForPersist,
  reorderLanes,
  resolveBoardLanes,
  sortJiraIssues,
} from "./jiraBoard";
import type { JiraDashboardIssue } from "./types";

function issue(
  partial: Partial<JiraDashboardIssue> & Pick<JiraDashboardIssue, "issueKey">,
): JiraDashboardIssue {
  return {
    summary: "",
    status: "To Do",
    assignee: "Ada",
    priority: "",
    priorityRank: null,
    estimateSeconds: null,
    totalSpentSeconds: null,
    browseUrl: "",
    taskId: null,
    taskName: null,
    timetroveBookedMs: null,
    ...partial,
  };
}

describe("filterJiraIssues", () => {
  const issues = [
    issue({ issueKey: "TT-1", summary: "Fix checkout" }),
    issue({ issueKey: "TT-2", summary: "Upgrade Node" }),
  ];

  it("matches key and summary case-insensitively", () => {
    expect(filterJiraIssues(issues, "tt-1").map((i) => i.issueKey)).toEqual([
      "TT-1",
    ]);
    expect(filterJiraIssues(issues, "node").map((i) => i.issueKey)).toEqual([
      "TT-2",
    ]);
  });

  it("returns all when query empty", () => {
    expect(filterJiraIssues(issues, "  ")).toHaveLength(2);
  });
});

describe("sortJiraIssues", () => {
  it("sorts priority by rank then name", () => {
    const issues = [
      issue({ issueKey: "A", priority: "Low", priorityRank: 4 }),
      issue({ issueKey: "B", priority: "Highest", priorityRank: 1 }),
      issue({ issueKey: "C", priority: "Medium", priorityRank: 3 }),
    ];
    expect(
      sortJiraIssues(issues, "priority", "asc").map((i) => i.issueKey),
    ).toEqual(["B", "C", "A"]);
  });

  it("sorts text columns", () => {
    const issues = [
      issue({ issueKey: "B-2", summary: "Zebra" }),
      issue({ issueKey: "A-1", summary: "Apple" }),
    ];
    expect(
      sortJiraIssues(issues, "summary", "asc").map((i) => i.issueKey),
    ).toEqual(["A-1", "B-2"]);
  });
});

describe("board lanes", () => {
  it("resolveBoardLanes prefers saved order then alphabetical extras", () => {
    const issues = [
      issue({ issueKey: "1", status: "In Review" }),
      issue({ issueKey: "2", status: "To Do" }),
      issue({ issueKey: "3", status: "Blocked" }),
    ];
    expect(resolveBoardLanes(issues, ["To Do", "In Progress", "In Review"])).toEqual([
      "To Do",
      "In Review",
      "Blocked",
    ]);
  });

  it("reorderLanes moves a lane", () => {
    expect(reorderLanes(["A", "B", "C"], 0, 2)).toEqual(["B", "C", "A"]);
  });

  it("mergeLaneOrderForPersist keeps empty saved lanes", () => {
    expect(
      mergeLaneOrderForPersist(["In Progress", "To Do"], [
        "To Do",
        "In Progress",
        "In Review",
      ]),
    ).toEqual(["In Progress", "To Do", "In Review"]);
  });
});
