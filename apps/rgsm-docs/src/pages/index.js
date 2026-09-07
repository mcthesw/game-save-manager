import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Screenshot from '@site/src/components/Screenshot';

import Heading from '@theme/Heading';
import styles from './index.module.css';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/intro">
            快速入门
          </Link>
          <Link className="button button--outline button--lg" to="/docs/extras/cloud">
            配置云同步
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description="保存重要进度，随时恢复旧存档。游戏存档管理器 1.9 使用指南与下载入口。">
      <HomepageHeader />
      <main>
        <HomepageFeatures />
        <div className="container">
          <Screenshot src="/img/guide/snapshots.png" alt="游戏存档管理器 1.9 中文界面" />
        </div>
      </main>
    </Layout>
  );
}
