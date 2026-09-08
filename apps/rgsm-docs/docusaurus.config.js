// @ts-check
// `@type` JSDoc annotations allow editor autocompletion and type checking
// (when paired with `@ts-check`).
// There are various equivalent ways to declare your Docusaurus config.
// See: https://docusaurus.io/docs/api/docusaurus-config

import {themes as prismThemes} from 'prism-react-renderer';

const isEnglish = process.env.DOCUSAURUS_CURRENT_LOCALE === 'en';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: isEnglish ? 'Game Save Manager' : '游戏存档管理器',
  tagline: isEnglish ? 'A simple, open-source game save manager with cloud sync' : '一个简单易用的开源存档管理工具，兼具云同步功能',
  favicon: 'img/rgsm.ico',

  // Set the production url of your site here
  url: 'https://help.sworld.club',
  // Set the /<baseUrl>/ pathname under which your site is served
  // For GitHub pages deployment, it is often '/<projectName>/'
  baseUrl: '/',

  // GitHub pages deployment config.
  // If you aren't using GitHub pages, you don't need these.
  organizationName: 'mcthesw',
  projectName: 'game-save-manager',

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'zh-CN',
    locales: ['zh-CN', 'en'],
    localeConfigs: {
      'zh-CN': {label: '简体中文', htmlLang: 'zh-CN'},
      en: {label: 'English', htmlLang: 'en'},
    },
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          editLocalizedFiles: true,
          editUrl:
            'https://github.com/mcthesw/game-save-manager/edit/dev/apps/rgsm-docs/',
        },
        blog: {
          showReadingTime: true,
          editLocalizedFiles: true,
          onUntruncatedBlogPosts: 'ignore',
          editUrl:
            'https://github.com/mcthesw/game-save-manager/edit/dev/apps/rgsm-docs/',
        },
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      // Replace with your project's social card
      image: 'img/guide/snapshots.png',
      navbar: {
        title: '存档管理器',
        logo: {
          alt: '游戏存档管理器',
          src: 'img/rgsm.png',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'tutorialSidebar',
            position: 'left',
            label: '帮助文档',
          },
          {
            type: 'docSidebar',
            sidebarId: 'developerSidebar',
            label: '开发者指南',
            position: 'left',
          },
          {to: '/blog', label: '更新日志', position: 'left'},
          {type: 'localeDropdown', position: 'right'},
          {
            href: 'https://github.com/mcthesw/game-save-manager',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: '文档',
            items: [
              {
                label: '使用文档',
                to: '/docs/intro',
              },
              {
                label: '开发者指南',
                to: '/docs/developers',
              },
            ],
          },
          {
            title: '社区',
            items: [
              {
                label: '软件主页',
                href: 'https://game.sworld.club',
              },
              {
                label: '讨论区',
                href: 'https://github.com/mcthesw/game-save-manager/discussions',
              },
              {
                label: 'Bilibili账号',
                href: 'https://space.bilibili.com/4087637',
              },
            ],
          },
          {
            title: '源代码',
            items: [
              {
                label: '项目源代码',
                href: 'https://github.com/mcthesw/game-save-manager',
              },
              {
                label: '文档源代码',
                href: 'https://github.com/mcthesw/game-save-manager/tree/dev/apps/rgsm-docs',
              },
            ],
          },
        ],
        copyright: `存档管理器是由 Sworld 和其它贡献者们共同开发的开源软件，服务于广大游戏玩家`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
      },
    }),
};

export default config;
