// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  tutorialSidebar: [
    'intro',
    {
      type: 'category',
      label: '游戏管理',
      link: {type: 'generated-index', slug: '/category/进阶教程'},
      items: ['games/add', 'games/paths', 'games/favorites'],
    },
    {
      type: 'category',
      label: '备份与恢复',
      items: ['backups/snapshots', 'extras/mechanism', 'backups/automatic', 'backups/shortcuts'],
    },
    {
      type: 'category',
      label: '云同步',
      items: ['extras/cloud', 'cloud/modes', 'cloud/progress', 'cloud/copies'],
    },
    {
      type: 'category',
      label: '设置与维护',
      items: ['settings', 'help/upgrade', 'extras/json', 'help/index'],
    },
    'issues',
    {
      type: 'category',
      label: '参与贡献',
      link: {type: 'generated-index'},
      items: ['contribute/develop', 'contribute/document', 'contribute/translate'],
    },
    'about',
  ],

};

export default sidebars;
