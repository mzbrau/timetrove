import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../shared/api";
import type {
  GithubActivityDigest,
  GithubCommitSummary,
  GithubPrSummary,
} from "../../shared/types";

const REPO_PALETTE = [
  "#E4572E",
  "#F3A712",
  "#17BEBB",
  "#2E86AB",
  "#A23B72",
  "#76B041",
  "#D64045",
  "#5C80BC",
  "#2A9D8F",
  "#E76F51",
  "#9B5DE5",
  "#00BBF9",
  "#F15BB5",
  "#00F5D4",
  "#9C6644",
  "#6D597A",
];

function repoAccent(repo: string): string {
  let h = 0;
  for (let i = 0; i < repo.length; i++) {
    h = (h * 31 + repo.charCodeAt(i)) >>> 0;
  }
  return REPO_PALETTE[h % REPO_PALETTE.length] ?? REPO_PALETTE[0];
}

function fmtWhen(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function relativeAge(iso: string): string {
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "—";
  const diff = Math.max(0, Date.now() - t);
  const mins = Math.floor(diff / 60_000);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 48) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days}d ago`;
  const weeks = Math.floor(days / 7);
  return `${weeks}w ago`;
}

function checkLabel(status: string): string {
  switch (status.toUpperCase()) {
    case "SUCCESS":
      return "Checks pass";
    case "FAILURE":
    case "ERROR":
      return "Checks fail";
    case "PENDING":
    case "EXPECTED":
      return "Checks pending";
    default:
      return status ? status.replace(/_/g, " ") : "No checks";
  }
}

function checkClass(status: string): string {
  switch (status.toUpperCase()) {
    case "SUCCESS":
      return "gh-pill-ok";
    case "FAILURE":
    case "ERROR":
      return "gh-pill-bad";
    case "PENDING":
    case "EXPECTED":
      return "gh-pill-warn";
    default:
      return "gh-pill-muted";
  }
}

function reviewLabel(decision: string): string {
  switch (decision.toUpperCase()) {
    case "APPROVED":
      return "Approved";
    case "CHANGES_REQUESTED":
      return "Changes requested";
    case "REVIEW_REQUIRED":
      return "Review required";
    case "":
      return "No decision";
    default:
      return decision.replace(/_/g, " ");
  }
}

function reviewClass(decision: string): string {
  switch (decision.toUpperCase()) {
    case "APPROVED":
      return "gh-pill-ok";
    case "CHANGES_REQUESTED":
      return "gh-pill-bad";
    case "REVIEW_REQUIRED":
      return "gh-pill-warn";
    default:
      return "gh-pill-muted";
  }
}

function openUrl(url: string) {
  if (!url) return;
  void api.openExternalUrl(url).catch(() => {});
}

function PrRow({ pr, emphasizeNeedsReview }: { pr: GithubPrSummary; emphasizeNeedsReview: boolean }) {
  const accent = repoAccent(pr.repoFullName);
  const needs = emphasizeNeedsReview && pr.needsMyReview;
  return (
    <button
      type="button"
      className={`gh-pr-row${needs ? " gh-pr-needs" : ""}`}
      style={{ ["--gh-accent" as string]: accent }}
      onClick={() => openUrl(pr.url)}
    >
      <span className="gh-pr-accent" aria-hidden />
      <span className="gh-pr-main">
        <span className="gh-pr-title-row">
          <span className="gh-pr-title">{pr.title}</span>
          <span className="gh-pr-num mono muted">#{pr.number}</span>
        </span>
        <span className="gh-pr-meta">
          <span className="gh-repo" style={{ color: accent }}>
            {pr.repoFullName}
          </span>
          <span className="muted">·</span>
          <span className="muted" title={fmtWhen(pr.createdAt)}>
            {relativeAge(pr.createdAt)}
          </span>
          {needs && <span className="gh-badge-needs">Needs your review</span>}
        </span>
        <span className="gh-pr-pills">
          <span className={`gh-pill ${checkClass(pr.checkStatus)}`}>
            {checkLabel(pr.checkStatus)}
          </span>
          <span className={`gh-pill ${reviewClass(pr.reviewDecision)}`}>
            {reviewLabel(pr.reviewDecision)}
          </span>
          <span className="gh-pill gh-pill-muted">
            {pr.unresolvedCommentCount === 0
              ? "No open threads"
              : `${pr.unresolvedCommentCount} unresolved`}
          </span>
          <span className="gh-pill gh-pill-muted">{pr.reviewsSummary}</span>
        </span>
      </span>
    </button>
  );
}

function CommitRow({ commit }: { commit: GithubCommitSummary }) {
  const accent = repoAccent(commit.repoFullName);
  const short = commit.sha.slice(0, 7);
  return (
    <button
      type="button"
      className="gh-commit-row"
      style={{ ["--gh-accent" as string]: accent }}
      onClick={() => openUrl(commit.url)}
    >
      <span className="gh-pr-accent" aria-hidden />
      <span className="gh-commit-main">
        <span className="gh-commit-msg">{commit.message || "(no message)"}</span>
        <span className="gh-pr-meta">
          <span className="gh-repo" style={{ color: accent }}>
            {commit.repoFullName}
          </span>
          <span className="muted">·</span>
          <span className="mono muted">{short}</span>
          <span className="muted">·</span>
          <span className="muted" title={fmtWhen(commit.committedAt)}>
            {relativeAge(commit.committedAt)}
          </span>
        </span>
      </span>
    </button>
  );
}

export function GitHubPanel() {
  const [digest, setDigest] = useState<GithubActivityDigest | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedDiag, setCopiedDiag] = useState(false);

  const load = useCallback(async () => {
    try {
      const d = await api.getGithubActivity();
      setDigest(d);
      setError(d?.lastError ?? null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen("github-activity-updated", () => {
      void load();
    })
      .then((fn) => {
        if (disposed) {
          fn();
          return;
        }
        unlisten = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [load]);

  const onRefresh = async () => {
    setRefreshing(true);
    setError(null);
    try {
      const d = await api.refreshGithubActivity();
      setDigest(d);
      setError(d?.lastError ?? null);
    } catch (e) {
      setError(String(e));
      void load();
    } finally {
      setRefreshing(false);
    }
  };

  const authored = digest?.authoredPrs ?? [];
  const review = digest?.reviewPrs ?? [];
  const commits = digest?.recentCommits ?? [];
  const needsCount = review.filter((p) => p.needsMyReview).length;
  const diagnostics = digest?.diagnostics ?? [];
  const listsEmpty = authored.length === 0 && review.length === 0 && commits.length === 0;

  const copyDiagnostics = async () => {
    if (diagnostics.length === 0) return;
    try {
      await navigator.clipboard.writeText(diagnostics.join("\n"));
      setCopiedDiag(true);
      window.setTimeout(() => setCopiedDiag(false), 1500);
    } catch {
      setCopiedDiag(false);
    }
  };

  return (
    <div className="github">
      <div className="github-toolbar">
        <div>
          <h2 className="github-heading">GitHub</h2>
          <p className="github-sync-meta muted">
            {digest?.username ? `@${digest.username} · ` : null}
            Last synced {fmtWhen(digest?.lastSyncedAt ?? digest?.fetchedAt)}
          </p>
        </div>
        <button
          type="button"
          className="btn"
          disabled={refreshing || loading}
          onClick={() => void onRefresh()}
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {error && <p className="error-text">{error}</p>}

      {diagnostics.length > 0 && (
        <details
          key={digest?.lastSyncedAt ?? digest?.fetchedAt ?? "diag"}
          className="github-diagnostics"
          defaultOpen={listsEmpty}
        >
          <summary className="github-diagnostics-summary">
            <span>Fetch diagnostics</span>
            <button
              type="button"
              className="btn github-diagnostics-copy"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                void copyDiagnostics();
              }}
            >
              {copiedDiag ? "Copied" : "Copy"}
            </button>
          </summary>
          <pre className="github-diagnostics-log mono">{diagnostics.join("\n")}</pre>
        </details>
      )}

      {loading && !digest ? (
        <p className="github-empty">Loading GitHub activity…</p>
      ) : (
        <>
          <section className="github-section">
            <h3>
              PRs you opened
              <span className="github-count mono muted">{authored.length}</span>
            </h3>
            {authored.length === 0 ? (
              <p className="github-empty">No open pull requests authored by you.</p>
            ) : (
              <div className="github-list">
                {authored.map((pr) => (
                  <PrRow key={pr.url} pr={pr} emphasizeNeedsReview={false} />
                ))}
              </div>
            )}
          </section>

          <section className="github-section">
            <h3>
              PRs to review
              <span className="github-count mono muted">{review.length}</span>
              {needsCount > 0 && (
                <span className="github-needs-count">{needsCount} need you</span>
              )}
            </h3>
            {review.length === 0 ? (
              <p className="github-empty">No open pull requests awaiting you as reviewer.</p>
            ) : (
              <div className="github-list">
                {review.map((pr) => (
                  <PrRow key={pr.url} pr={pr} emphasizeNeedsReview />
                ))}
              </div>
            )}
          </section>

          <section className="github-section">
            <h3>
              Recent commits
              <span className="github-count mono muted">{commits.length}</span>
            </h3>
            {commits.length === 0 ? (
              <p className="github-empty">No recent commits found for your account.</p>
            ) : (
              <div className="github-list">
                {commits.map((c) => (
                  <CommitRow key={`${c.repoFullName}-${c.sha}`} commit={c} />
                ))}
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
