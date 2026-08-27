'use client';

import styles from './DesktopDemo.module.css';

export function DesktopDemo() {
  return (
    <div className={styles.stage}>
      <div className={styles.viewport}>
        <iframe
          title="Cursor BYOK 真实产品界面演示"
          src="/product-demo/demo/index.html"
          className={styles.demo}
        />
      </div>
      <p>真实桌面端组件 · 使用隔离的 Mock 数据，可直接操作菜单、筛选和设置</p>
    </div>
  );
}
