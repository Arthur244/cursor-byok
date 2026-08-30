import { useEffect, useRef, useState } from "react";
import type { PluginRuntimePhase, PluginRuntimeStatus } from "../../shared/api";
import { PageContent } from "../../shell/layout/PageContent";
import { appStore, useAppStore } from "../../shared/store/appStore";
import { Button } from "../../shared/ui/Button";
import { Modal } from "../../shared/ui/Modal";
import { TitledCard } from "../../shared/ui/TitledCard";
import styles from "./PluginManagementPage.module.scss";

export function PluginManagementPage() {
  const { pluginRuntime } = useAppStore();
  const [progressOpen, setProgressOpen] = useState(false);
  const [starting, setStarting] = useState(false);
  const cancelRequested = useRef(false);

  useEffect(() => {
    if (!pluginRuntime) void appStore.refreshPluginRuntime();
  }, [pluginRuntime]);

  useEffect(() => {
    if (pluginRuntime?.state !== "initializing") return;
    if (!cancelRequested.current) setProgressOpen(true);
    const timer = window.setInterval(() => void appStore.refreshPluginRuntime(), 300);
    return () => window.clearInterval(timer);
  }, [pluginRuntime?.state]);

  const initialize = async () => {
    if (starting) return;
    cancelRequested.current = false;
    setStarting(true);
    setProgressOpen(true);
    const status = await appStore.initializePluginRuntime();
    setStarting(false);
    if (!status) {
      setProgressOpen(false);
    } else if (cancelRequested.current && status.state === "initializing") {
      void appStore.cancelPluginRuntimeInitialization();
    }
  };

  const closeProgress = () => {
    setProgressOpen(false);
    cancelRequested.current = true;
    if (pluginRuntime?.state === "initializing") {
      void appStore.cancelPluginRuntimeInitialization();
    }
  };

  const content = pluginRuntime?.state === "ready"
    ? <RuntimeReady status={pluginRuntime} />
    : <RuntimeGate status={pluginRuntime} starting={starting} onInitialize={() => void initialize()} />;

  return <>
    <PageContent
      title={t("插件管理")}
      sections={[{ key: "plugin-runtime", estimatedHeight: 320, content }]}
    />
    <RuntimeProgressModal
      open={progressOpen}
      status={pluginRuntime}
      starting={starting}
      onClose={closeProgress}
    />
  </>;
}

function RuntimeGate({ status, starting, onInitialize }: { status: PluginRuntimeStatus | null; starting: boolean; onInitialize: () => void }) {
  const checking = status === null;
  const initializing = starting || status?.state === "initializing";
  const failed = status?.state === "failed";
  const unsupported = status?.state === "unsupported";
  const title = checking
    ? t("正在检查插件运行时")
    : failed
      ? t("插件运行时初始化失败")
      : unsupported
        ? t("当前系统不支持插件运行时")
        : t("需要先初始化插件运行时");
  const description = failed
    ? t("请重试初始化")
    : unsupported
      ? status.error ?? t("当前操作系统或 CPU 架构暂不受支持")
      : t("初始化将下载并安装插件运行时。");

  return <div className={styles.gate}>
    <strong>{title}</strong>
    <span>{description}</span>
    {!unsupported && <Button variant="primary" disabled={checking || initializing} onClick={onInitialize}>
      {checking ? t("检查中…") : initializing ? t("初始化中…") : failed ? t("重新初始化插件") : t("初始化插件")}
    </Button>}
  </div>;
}

function RuntimeReady({ status }: { status: PluginRuntimeStatus }) {
  return <div className={styles.page}>
    <TitledCard title={t("插件运行时")}>
      <div className={styles.runtimeDetails}>
        <div><strong>{t("状态")}</strong><span className={styles.ready}>{t("已就绪")}</span></div>
        <div><strong>{t("插件运行时版本")}</strong><span>{status.version}</span></div>
        <div><strong>{t("运行平台")}</strong><span>{status.target}</span></div>
      </div>
    </TitledCard>
  </div>;
}

function RuntimeProgressModal({ open, status, starting, onClose }: { open: boolean; status: PluginRuntimeStatus | null; starting: boolean; onClose: () => void }) {
  const initializing = starting || status?.state === "initializing";
  const downloaded = status?.downloaded_bytes ?? 0;
  const total = status?.total_bytes ?? null;
  const percent = total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
  const stage = status?.state === "ready"
    ? t("插件运行时初始化完成")
    : status?.state === "failed"
      ? t("插件运行时初始化失败")
      : phaseText(status?.phase ?? null);

  return <Modal
    open={open}
    title={t("初始化插件运行时")}
    closeLabel={status?.state === "ready" ? t("完成") : initializing ? t("取消") : t("关闭")}
    onClose={onClose}
  >
    <div className={styles.progressContent} aria-live="polite">
      <strong>{stage}</strong>
      {status?.phase === "downloading" && <>
        <progress
          aria-label={t("下载进度")}
          value={percent ?? undefined}
          max={100}
        />
        <span>
          {total ? t("已下载 {downloaded} / {total}", { downloaded: formatBytes(downloaded), total: formatBytes(total) }) : t("已下载 {downloaded}", { downloaded: formatBytes(downloaded) })}
        </span>
      </>}
      {status?.state === "failed" && <span className={styles.error}>{t("请重试初始化")}</span>}
      {status?.state === "ready" && <span>{t("插件运行时 {version} 已安装，可以开始使用插件。", { version: status.version })}</span>}
    </div>
  </Modal>;
}

function phaseText(phase: PluginRuntimePhase | null) {
  switch (phase) {
    case "checking": return t("正在检查插件运行时");
    case "downloading": return t("正在下载插件运行时");
    case "verifying": return t("正在验证插件运行时下载文件");
    case "installing": return t("正在安装插件运行时");
    case "validating": return t("正在验证插件运行时");
    default: return t("正在准备插件运行时");
  }
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : value.toFixed(0)} ${units[unit]}`;
}
