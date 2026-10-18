import { themes as prismThemes } from "prism-react-renderer";
import type { Config } from "@docusaurus/types";
import type * as Preset from "@docusaurus/preset-classic";

const repoUrl = "https://github.com/mzbrau/Time-Keeper";

const config: Config = {
  title: "TimeTrove",
  tagline: "Local-first time tracking for developers",
  favicon: "img/favicon.png",

  url: "https://mzbrau.github.io",
  baseUrl: "/Time-Keeper/",
  organizationName: "mzbrau",
  projectName: "Time-Keeper",
  trailingSlash: false,

  onBrokenLinks: "throw",
  markdown: {
    hooks: { onBrokenMarkdownLinks: "throw" },
  },

  i18n: { defaultLocale: "en", locales: ["en"] },

  headTags: [
    { tagName: "link", attributes: { rel: "preconnect", href: "https://fonts.googleapis.com" } },
    {
      tagName: "link",
      attributes: { rel: "preconnect", href: "https://fonts.gstatic.com", crossorigin: "anonymous" },
    },
    {
      tagName: "link",
      attributes: {
        rel: "stylesheet",
        href: "https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:ital,wght@0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@500;600&display=swap",
      },
    },
  ],

  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: "./sidebars.ts",
          editUrl: `${repoUrl}/edit/main/website/`,
        },
        blog: false,
        theme: { customCss: "./src/css/custom.css" },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: "img/screenshots/tasks-overview.png",
    colorMode: {
      defaultMode: "dark",
      respectPrefersColorScheme: false,
    },
    navbar: {
      title: "TimeTrove",
      logo: { alt: "TimeTrove logo", src: "img/timetrove-logo.png" },
      items: [
        { type: "docSidebar", sidebarId: "docs", position: "left", label: "Docs" },
        { to: "/docs/integrations/jira", label: "Integrations", position: "left" },
        { href: `${repoUrl}/releases`, label: "Download", position: "right" },
        { href: repoUrl, label: "GitHub", position: "right" },
      ],
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Docs",
          items: [
            { label: "Getting started", to: "/docs/getting-started" },
            { label: "Gap attribution", to: "/docs/tracking/gap-attribution" },
            { label: "Keyboard shortcuts", to: "/docs/keyboard-shortcuts" },
          ],
        },
        {
          title: "Project",
          items: [
            { label: "Releases", href: `${repoUrl}/releases` },
            { label: "Source", href: repoUrl },
            { label: "Issues", href: `${repoUrl}/issues` },
          ],
        },
      ],
      copyright: `TimeTrove. Your time stays on your machine.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.vsDark,
      additionalLanguages: ["bash", "json"],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
