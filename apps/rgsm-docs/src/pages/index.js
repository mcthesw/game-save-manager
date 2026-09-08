import clsx from 'clsx';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
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
            <Translate id="home.getStarted">快速入门</Translate>
          </Link>
          <Link className="button button--outline button--lg" to="/docs/extras/cloud">
            <Translate id="home.cloudSync">配置云同步</Translate>
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const {siteConfig, i18n} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description={translate({id: 'home.description', message: '保存重要进度，随时恢复旧存档。游戏存档管理器 1.9 使用指南与下载入口。'})}>
      <HomepageHeader />
      <main>
        <HomepageFeatures />
        <div className="container">
          <Screenshot src={i18n.currentLocale === 'en' ? '/img/guide/en/snapshots.png' : '/img/guide/snapshots.png'} alt={translate({id: 'home.screenshot', message: '游戏存档管理器 1.9 中文界面'})} />
        </div>
      </main>
    </Layout>
  );
}
