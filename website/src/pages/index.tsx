import React from "react";
import clsx from "clsx";
import Link from "@docusaurus/Link";
import useBaseUrl from "@docusaurus/useBaseUrl";
import Layout from "@theme/Layout";
import LiveTracker from "@site/src/components/LiveTracker";
import styles from "./index.module.css";

const RELEASES = "https://github.com/mzbrau/Time-Keeper/releases";

function Shot({
  name,
  alt,
  className,
  style,
}: {
  name: string;
  alt: string;
  className?: string;
  style?: React.CSSProperties;
}) {
  return (
    <img
      className={clsx(styles.shot, className)}
      style={style}
      src={useBaseUrl(`/img/screenshots/${name}.png`)}
      alt={alt}
      loading="lazy"
      decoding="async"
    />
  );
}

type Focus = { x: number; y: number; w: number };

/**
 * A framed region of a screenshot. `focus` is the region's top-left corner and width as
 * fractions of the image; the frame's aspect ratio decides how much height shows.
 */
function Crop({
  name,
  alt,
  focus = { x: 0, y: 0, w: 1 },
  aspect = "16 / 10",
  className,
}: {
  name: string;
  alt: string;
  focus?: Focus;
  aspect?: string;
  className?: string;
}) {
  return (
    <div className={clsx(styles.crop, className)} style={{ aspectRatio: aspect }}>
      <Shot
        name={name}
        alt={alt}
        style={{
          width: `${100 / focus.w}%`,
          transform: `translate(${-focus.x * 100}%, ${-focus.y * 100}%)`,
        }}
      />
    </div>
  );
}

function Hero() {
  return (
    <header className={styles.hero}>
      <div className={styles.heroText}>
        <img className={styles.mark} src={useBaseUrl("/img/timetrove-mark.png")} alt="" width={88} height={88} />
        <h1 className={styles.wordmark}>TimeTrove</h1>
        <p className={styles.lede}>
          A time tracker that sits in a small bar on your screen, asks where idle time went instead of guessing, and
          turns your week into a timesheet. Everything stays in one file on your machine.
        </p>
        <div className={styles.ctas}>
          <Link className={clsx(styles.btn, styles.btnPrimary)} to="/docs/getting-started">
            Get started
          </Link>
          <Link className={clsx(styles.btn, styles.btnGhost)} href={RELEASES}>
            Download for macOS or Windows
          </Link>
        </div>
      </div>

      <div className={styles.stage}>
        <div className={styles.window}>
          <Shot name="tasks-overview" alt="TimeTrove main window showing the task list and a task linked to Jira" />
        </div>
        <LiveTracker
          className={styles.floatingBar}
          task="Data export service"
          color="#8b5cf6"
          startSeconds={7 * 3600 + 59 * 60 + 20}
        />
      </div>
    </header>
  );
}

const steps = [
  {
    title: "Track",
    body: "Press Cmd/Ctrl+Shift+T anywhere, type a few letters, hit Enter. You're timing the new task, and the old one is closed off.",
    shot: "switcher-search",
    alt: "Task switcher filtering tasks by the letters rev",
  },
  {
    title: "Correct",
    body: "Walked away? When you return, one prompt asks where the gap goes: 1 keeps it, 2 moves it, 3 throws it out.",
    shot: "gap-prompt",
    alt: "Gap prompt showing 47 minutes and three numbered choices",
  },
  {
    title: "Report",
    body: "A Monday-start grid of tasks by day, with totals. Copy it into your timesheet tool or export CSV.",
    shot: "timesheet-week",
    alt: "Weekly timesheet with tasks as rows and days as columns",
    focus: { x: 0.03, y: 0.254, w: 0.58 },
  },
];

function Loop() {
  return (
    <section className={styles.section}>
      <h2 className={styles.sectionTitle}>Three steps, every day</h2>
      <ol className={styles.steps}>
        {steps.map((s, i) => (
          <li key={s.title} className={styles.step}>
            <div className={styles.stepHead}>
              <span className={styles.stepNum}>{i + 1}</span>
              <h3>{s.title}</h3>
            </div>
            <p>{s.body}</p>
            <Crop
              name={s.shot}
              alt={s.alt}
              aspect="4 / 3.3"
              focus={s.focus}
              className={styles.stepShot}
            />
          </li>
        ))}
      </ol>
    </section>
  );
}

/** Tracker shots are 2x; widths are relative to the widest (780px) so all bars share one scale. */
const barStates = [
  { name: "tracker-tracking", label: "Tracking", width: 620 },
  { name: "tracker-hover-stop", label: "Hover to stop, or open the Jira issue", width: 620 },
  { name: "tracker-copilot", label: "With Copilot usage", width: 780 },
  { name: "tracker-goal-reached", label: "Daily goal reached", width: 780 },
  { name: "tracker-stopped", label: "Not tracking", width: 780 },
];

function Features() {
  return (
    <section className={styles.section}>
      <h2 className={styles.sectionTitle}>Built around how a workday actually goes</h2>
      <div className={styles.bento}>
        <article className={clsx(styles.tile, styles.tileBar)}>
          <h3>Always in view</h3>
          <p>
            The bar shows what you're on and today's total. The clock turns gold at your daily goal, and "Not tracking"
            is impossible to miss.
          </p>
          <ul className={styles.barStack}>
            {barStates.map((b) => (
              <li key={b.name}>
                <Shot name={b.name} alt={`Tracker bar: ${b.label}`} style={{ width: `${(b.width / 780) * 100}%` }} />
                <span>{b.label}</span>
              </li>
            ))}
          </ul>
          <Link to="/docs/tracking/tracker-bar">The tracker bar</Link>
        </article>

        <article className={clsx(styles.tile, styles.tileWide)}>
          <h3>Fix a day by dragging</h3>
          <p>Resize entries on the timeline, drag across empty time to add one, then split, move or annotate the rest.</p>
          <Crop
            name="entries"
            alt="Time entries for one day with a timeline and an editable table"
            focus={{ x: 0.03, y: 0.244, w: 0.95 }}
            aspect="16 / 8"
          />
          <Link to="/docs/timeline-and-entries">Timeline and entries</Link>
        </article>

        <article className={styles.tile}>
          <h3>Tasks with context</h3>
          <p>Colour, notes, and an optional Jira issue with estimate and time booked so far.</p>
          <Crop
            name="task-detail-jira"
            alt="Task detail with Jira card and previous tickets"
            focus={{ x: 0.285, y: 0.404, w: 0.5 }}
            aspect="4 / 3"
          />
          <Link to="/docs/tasks">Tasks</Link>
        </article>

        <article className={styles.tile}>
          <h3>Snooze for long meetings</h3>
          <p>Keep the clock on the current task through a workshop without idle prompts.</p>
          <Shot name="snooze-picker" alt="Snooze picker with hours and minutes" className={styles.small} />
          <Link to="/docs/tracking/snooze">Snooze</Link>
        </article>

        <article className={clsx(styles.tile, styles.tileFull)}>
          <h3>Your week at a glance</h3>
          <p>Every day on one 24-hour scale, coloured by task, so a short Friday or a late Tuesday stands out.</p>
          <Crop
            name="day-timeline"
            alt="Week timeline with one row per day"
            focus={{ x: 0.03, y: 0.324, w: 0.94 }}
            aspect="11 / 5"
          />
          <Link to="/docs/timesheet">Timesheet</Link>
        </article>
      </div>
    </section>
  );
}

function InsightsBand() {
  return (
    <section className={styles.band}>
      <div className={styles.bandInner}>
        <div className={styles.bandText}>
          <h2>See the shape of your year</h2>
          <p>
            Pick a week, a month or the last twelve months. Insights shows average start and finish times, lunch length,
            how often you hit your goal, and where the hours went by task, weekday and hour of day.
          </p>
          <Link className={clsx(styles.btn, styles.btnGhost)} to="/docs/insights">
            Explore Insights
          </Link>
        </div>
        <div className={styles.bandShots}>
          <Shot name="insights-year" alt="Insights for the last year: totals, averages and hours by weekday" />
          <Crop
            name="insights-tasks"
            alt="Insights: time by task as a donut chart"
            focus={{ x: 0.04, y: 0.234, w: 0.46 }}
            aspect="20 / 18"
            className={styles.bandBack}
          />
        </div>
      </div>
    </section>
  );
}

const integrations = [
  {
    name: "Jira Cloud",
    body: "Link a task to an issue and TimeTrove posts your time as worklogs, every hour and whenever you switch.",
    shot: "jira-dashboard",
    focus: { x: 0.03, y: 0.234, w: 0.62 },
    aspect: "16 / 9",
    to: "/docs/integrations/jira",
  },
  {
    name: "GitHub",
    body: "Your open pull requests, the ones waiting on your review, and your last 20 commits in one tab.",
    shot: "github-activity",
    focus: { x: 0.03, y: 0.234, w: 0.56 },
    aspect: "16 / 9",
    to: "/docs/integrations/github",
  },
  {
    name: "GitHub Copilot",
    body: "Premium request usage against your monthly allowance, with a pace line and a percentage on the bar.",
    shot: "copilot-usage",
    focus: { x: 0.03, y: 0.234, w: 0.56 },
    aspect: "16 / 9",
    to: "/docs/integrations/copilot",
  },
];

function Integrations() {
  return (
    <section className={styles.section}>
      <h2 className={styles.sectionTitle}>Optional integrations</h2>
      <p className={styles.sectionLede}>All three are off until you add a token. Nothing is sent anywhere else.</p>
      <div className={styles.integrations}>
        {integrations.map((it) => (
          <Link key={it.name} to={it.to} className={styles.integration}>
            <Crop name={it.shot} alt={`${it.name} tab`} focus={it.focus} aspect={it.aspect} />
            <h3>{it.name}</h3>
            <p>{it.body}</p>
          </Link>
        ))}
      </div>
    </section>
  );
}

function LocalFirst() {
  return (
    <section className={clsx(styles.section, styles.local)}>
      <h2>Your time stays on your machine</h2>
      <p>
        No account, no server, no sync. TimeTrove keeps everything in a single SQLite file that you can back up, copy or
        delete.
      </p>
      <dl className={styles.paths}>
        <div>
          <dt>macOS</dt>
          <dd>~/Library/Application Support/com.mzbrau.timetrove/timetrove.db</dd>
        </div>
        <div>
          <dt>Windows</dt>
          <dd>%APPDATA%\com.mzbrau.timetrove\timetrove.db</dd>
        </div>
      </dl>
      <div className={styles.ctas}>
        <Link className={clsx(styles.btn, styles.btnPrimary)} to="/docs/getting-started">
          Get started
        </Link>
        <Link className={clsx(styles.btn, styles.btnGhost)} to="/docs/data-and-backup">
          Data and backup
        </Link>
      </div>
    </section>
  );
}

export default function Home(): React.ReactElement {
  return (
    <Layout
      title="Local-first time tracking for developers"
      description="TimeTrove is a keyboard-first desktop time tracker with idle-gap correction, a weekly timesheet and insights. Data stays local."
    >
      <main className={styles.page}>
        <Hero />
        <Loop />
        <Features />
        <InsightsBand />
        <Integrations />
        <LocalFirst />
      </main>
    </Layout>
  );
}
