import React from 'react';
import clsx from 'clsx';
import styles from './styles.module.css';

const iconProps = {
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 1.8,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
  'aria-hidden': true,
};

function PcIcon({className}) {
  return (
    <svg viewBox="0 0 24 24" className={className} {...iconProps}>
      <rect x="3" y="4" width="18" height="11" rx="1.5" />
      <path d="M12 15v4" />
      <path d="M8.5 19h7" />
    </svg>
  );
}

function HandheldIcon({className}) {
  return (
    <svg viewBox="0 0 24 24" className={className} {...iconProps}>
      <rect x="2" y="8" width="20" height="8.5" rx="4.25" />
      <rect x="10" y="10.2" width="4" height="4.1" rx="0.6" />
      <circle cx="6.2" cy="12.2" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="17.8" cy="12.2" r="1.2" fill="currentColor" stroke="none" />
    </svg>
  );
}

function CloudIcon({className}) {
  return (
    <svg viewBox="0 0 24 24" className={className} {...iconProps}>
      <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
    </svg>
  );
}

function LockIcon({className}) {
  return (
    <svg viewBox="0 0 24 24" className={className} {...iconProps}>
      <rect x="4" y="11" width="16" height="9.5" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  );
}

function ProtectedChip() {
  return (
    <span className={styles.chip}>
      <LockIcon className={styles.chipIcon} />
      先备份再覆盖
    </span>
  );
}

function Lane({kind, heads, children}) {
  return (
    <div className={clsx(styles.lane, styles[kind], styles[heads])}>
      <span className={styles.arrow} aria-hidden="true" />
      <span className={styles.laneLabel}>{children}</span>
    </div>
  );
}

function Node({icon: Icon, name, sub, highlighted, chip}) {
  return (
    <div className={clsx(styles.node, highlighted && styles.nodeHighlighted)}>
      <Icon className={styles.nodeIcon} />
      <span className={styles.nodeName}>{name}</span>
      <span className={styles.nodeSub}>{sub}</span>
      {chip}
    </div>
  );
}

const recordsLane = {
  kind: 'auto',
  heads: 'headsBoth',
  label: '自动：存档记录与当前进度',
};

const MODES = [
  {
    id: 'sync-mode-manual',
    title: '手动',
    tagline: '进度记录自动上云；存档文件的传输与应用完全由你决定时机。',
    left: [
      recordsLane,
      {kind: 'manual', heads: 'headsBoth', label: '手动：存档文件上传 / 下载'},
    ],
    right: [
      recordsLane,
      {kind: 'manual', heads: 'headsBoth', label: '手动：存档文件上传 / 下载'},
    ],
    protectedApply: false,
    notes: [
      <>自动：备份后同步存档记录与当前进度，其他设备能看到你玩到了哪里</>,
      <>手动：上传 / 下载存档文件，以及把存档应用（覆盖）到游戏</>,
      <>适合：偶尔上传存档，或想完全掌控每一次传输的时机</>,
    ],
  },
  {
    id: 'sync-mode-backup',
    title: '云备份 = 手动 + 自动上传',
    tagline: '本设备每产生一个新存档，就自动上传到云端，防止本设备数据丢失。',
    left: [
      recordsLane,
      {kind: 'new', heads: 'headsRight', label: '自动：新存档上传到云端'},
      {kind: 'manual', heads: 'headsLeft', label: '手动：需要时下载存档'},
    ],
    right: [
      recordsLane,
      {kind: 'new', heads: 'headsLeft', label: '自动：新存档上传到云端'},
      {kind: 'manual', heads: 'headsRight', label: '手动：需要时下载存档'},
    ],
    protectedApply: false,
    notes: [
      <>
        自动：记录同步之外，<strong>新存档自动上传到云端</strong>
        （图中蓝色，本模式新增）
      </>,
      <>手动：下载历史存档或其他设备的存档、把存档应用到游戏</>,
      <>适合：主要在一台设备上玩，希望云端始终留有最新备份</>,
    ],
  },
  {
    id: 'sync-mode-multi',
    title: '多设备同步 = 云备份 + 自动接续',
    tagline: '另一台设备进度领先时，自动下载并应用接续所需的存档；覆盖前先备份当前进度。',
    left: [
      recordsLane,
      {kind: 'auto', heads: 'headsRight', label: '自动：新存档上传到云端'},
      {kind: 'manual', heads: 'headsLeft', label: '手动：需要时下载存档'},
    ],
    right: [
      recordsLane,
      {kind: 'auto', heads: 'headsLeft', label: '自动：新存档上传到云端'},
      {kind: 'new', heads: 'headsRight', label: '自动：下载继续游戏所需的进度'},
    ],
    protectedApply: true,
    notes: [
      <>
        自动：另一台设备进度领先时，<strong>只下载继续游戏所需的那一个存档</strong>
        （图中蓝色，本模式新增），不会自动下载全部历史存档
      </>,
      <>先备份再覆盖：覆盖游戏存档前自动备份当前进度，不满意可随时回退</>,
      <>手动：下载全部历史存档；处理真正的进度分叉</>,
      <>适合：在电脑与掌机等多台设备之间接力同一款游戏</>,
    ],
  },
];

function ModePanel({mode}) {
  return (
    <section className={styles.panel} aria-labelledby={mode.id}>
      <h4 className={styles.panelTitle} id={mode.id}>
        {mode.title}
      </h4>
      <p className={styles.tagline}>{mode.tagline}</p>
      <div className={styles.diagram}>
        <Node icon={PcIcon} name="电脑" sub="设备 A" />
        <div className={styles.connector}>
          {mode.left.map((lane) => (
            <Lane key={lane.label} kind={lane.kind} heads={lane.heads}>
              {lane.label}
            </Lane>
          ))}
        </div>
        <Node icon={CloudIcon} name="云端" sub="WebDAV / S3" />
        <div className={styles.connector}>
          {mode.right.map((lane) => (
            <Lane key={lane.label} kind={lane.kind} heads={lane.heads}>
              {lane.label}
            </Lane>
          ))}
        </div>
        <Node
          icon={HandheldIcon}
          name="掌机"
          sub="设备 B"
          highlighted={mode.protectedApply}
          chip={mode.protectedApply ? <ProtectedChip /> : null}
        />
      </div>
      <ul className={styles.notes}>
        {mode.notes.map((note, i) => (
          <li key={i}>{note}</li>
        ))}
      </ul>
    </section>
  );
}

export default function CloudSyncModes() {
  return (
    <div className={styles.wrap}>
      <ul className={styles.legend}>
        <li>
          <span
            className={clsx(styles.swatch, styles.swatchAuto)}
            aria-hidden="true"
          />
          实线：自动进行
        </li>
        <li>
          <span
            className={clsx(styles.swatch, styles.swatchManual)}
            aria-hidden="true"
          />
          虚线：需要手动操作
        </li>
        <li>
          <span
            className={clsx(styles.swatch, styles.swatchNew)}
            aria-hidden="true"
          />
          蓝色：本模式新增
        </li>
        <li className={styles.legendChip}>
          <ProtectedChip />
          覆盖游戏存档前自动备份当前进度
        </li>
      </ul>
      <div className={styles.panels}>
        {MODES.map((mode) => (
          <ModePanel key={mode.id} mode={mode} />
        ))}
      </div>
    </div>
  );
}
