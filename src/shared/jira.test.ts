import { describe, expect, it } from "vitest";
import { jiraBrowseUrl, jiraPercentComplete } from "./jira";

describe("jiraBrowseUrl", () => {
  it("builds browse URL from https site and key", () => {
    expect(jiraBrowseUrl("https://example.atlassian.net/", "thing-9")).toBe(
      "https://example.atlassian.net/browse/THING-9",
    );
  });

  it("rejects non-https or empty key", () => {
    expect(jiraBrowseUrl("http://example.atlassian.net", "A-1")).toBeNull();
    expect(jiraBrowseUrl("https://example.atlassian.net", "")).toBeNull();
  });
});

describe("jiraPercentComplete", () => {
  it("returns null without estimate", () => {
    expect(jiraPercentComplete(null, 3600_000, 1800)).toBeNull();
    expect(jiraPercentComplete(0, 3600_000, 1800)).toBeNull();
  });

  it("prefers TimeTrove booked when present", () => {
    // 1h booked / 2h estimate = 50%
    expect(jiraPercentComplete(7200, 3_600_000, 0)).toBe(50);
  });

  it("falls back to Jira spent when unlinked", () => {
    // 30m spent / 1h estimate = 50%
    expect(jiraPercentComplete(3600, null, 1800)).toBe(50);
  });
});
