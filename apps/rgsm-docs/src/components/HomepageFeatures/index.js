import clsx from 'clsx';
import Heading from '@theme/Heading';
import Translate from '@docusaurus/Translate';
import styles from './styles.module.css';

const FeatureList = [
  {
    title: <Translate id="home.features.save.title">保存重要进度</Translate>,
    description: (
      <Translate id="home.features.save.description">
        手动备份或定时保存，给存档写一句描述，需要时轻松找到。
      </Translate>
    ),
  },
  {
    title: <Translate id="home.features.restore.title">恢复到想要的时刻</Translate>,
    description: (
      <Translate id="home.features.restore.description">
        查看存档列表和进度分支，选择要恢复的存档，覆盖前保留额外备份。
      </Translate>
    ),
  },
  {
    title: <Translate id="home.features.cloud.title">换台设备接着玩</Translate>,
    description: (
      <Translate id="home.features.cloud.description">
        连接自己的 WebDAV 或 S3 存储，在多台设备之间备份和恢复存档。
      </Translate>
    ),
  },
];

function Feature({title, description}) {
  return (
    <div className={clsx('col col--4')}>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
