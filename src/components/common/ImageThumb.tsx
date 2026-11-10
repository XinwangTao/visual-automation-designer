/**
 * ImageThumb — 图片缩略图。
 *
 * WebView 不能直接读图片库目录（`<img src="文件名">` 必然 404），缩略图
 * 统一走后端命令拿 data URL，并按 id+尺寸做内存缓存，避免每次重绘都走 IPC。
 */

import { useEffect, useState } from 'react';
import { getImageThumbnail } from '../../tauri/commands';

const cache = new Map<string, string>();

function cacheKey(id: string, maxSize: number): string {
  return `${id}:${maxSize}`;
}

/** 图片被替换/改过之后调用，让下次渲染重新取图。 */
export function invalidateImageThumb(id: string): void {
  for (const key of [...cache.keys()]) {
    if (key.startsWith(`${id}:`)) cache.delete(key);
  }
}

export interface ImageThumbProps {
  id: string;
  alt?: string;
  className?: string;
  /** 最长边像素，默认 320；详情大图用 1024。 */
  maxSize?: number;
}

export function ImageThumb({ id, alt = '', className, maxSize = 320 }: ImageThumbProps) {
  const key = cacheKey(id, maxSize);
  const [src, setSrc] = useState<string | null>(() => cache.get(key) ?? null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const cached = cache.get(key);
    if (cached) {
      setSrc(cached);
      setFailed(false);
      return;
    }
    let cancelled = false;
    setSrc(null);
    setFailed(false);
    getImageThumbnail(id, maxSize)
      .then((url) => {
        cache.set(key, url);
        if (!cancelled) setSrc(url);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [id, key, maxSize]);

  if (!src) {
    return (
      <div className={className} aria-hidden="true" data-thumb-state={failed ? 'failed' : 'loading'} />
    );
  }
  return <img className={className} src={src} alt={alt} draggable={false} />;
}

export default ImageThumb;
