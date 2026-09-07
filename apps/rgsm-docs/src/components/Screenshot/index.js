import React from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './styles.module.css';

export default function Screenshot({src, alt, caption}) {
  const url = useBaseUrl(src);
  return (
    <figure className={styles.figure}>
      <a className={styles.frame} href={url} target="_blank" rel="noreferrer" aria-label={`${alt}，查看原图`}>
        <img src={url} alt={alt} width="1280" height="800" loading="lazy" decoding="async" />
      </a>
      {caption && <figcaption>{caption}</figcaption>}
    </figure>
  );
}
