import type { SidebarsConfig } from "@docusaurus/plugin-content-docs";

const sidebars: SidebarsConfig = {
  docs: [
    "intro",
    "getting-started",
    "concepts",
    {
      type: "category",
      label: "Tracking",
      collapsed: false,
      items: [
        "tracking/tracker-bar",
        "tracking/switching-tasks",
        "tracking/gap-attribution",
        "tracking/snooze",
      ],
    },
    "tasks",
    "timeline-and-entries",
    "timesheet",
    "insights",
    {
      type: "category",
      label: "Integrations",
      collapsed: false,
      items: ["integrations/jira", "integrations/github", "integrations/copilot"],
    },
    "settings",
    "data-and-backup",
    "keyboard-shortcuts",
    "faq",
  ],
};

export default sidebars;
