import clsx from 'clsx';
import Heading from '@theme/Heading';
import styles from './styles.module.css';

const FeatureList = [
  {
    title: '保存重要进度',
    description: (
      <>
        手动备份或定时保存，给存档写一句描述，需要时轻松找到。
      </>
    ),
  },
  {
    title: '恢复到想要的时刻',
    description: (
      <>
        查看存档列表和进度分支，选择要恢复的存档，覆盖前保留额外备份。
      </>
    ),
  },
  {
    title: '换台设备接着玩',
    description: (
      <>
        连接自己的 WebDAV 或 S3 存储，在多台设备之间备份和恢复存档。
      </>
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
