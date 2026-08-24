import { useEffect, useMemo, useState } from "react";
import { api, type Model, type ProviderSelection, type TabSettings } from "../api";
import { CursorCaGate, CursorCaProvider, CursorModelGate, CursorModelProvider } from "../components/cursor/CursorGates";
import { CursorModelEditor, emptyCursorModelDraft, type CursorModelDraft } from "../components/cursor/CursorModelEditor";
import { CursorModelTestResult, type CursorModelTestState } from "../components/cursor/CursorModelTestResult";
import { TabSettingsCard } from "../components/cursor/TabSettingsCard";
import styles from "../components/cursor/CursorSettings.module.scss";
import { PageContent } from "../components/layout/PageContent";
import controls from "../components/ui/Controls.module.scss";
import { Icon } from "../components/ui/Icon";
import { Modal } from "../components/ui/Modal";
import { TitledCard } from "../components/ui/TitledCard";
import { TooltipTrigger } from "../components/ui/TooltipTrigger";
import { addIcon, claudeIcon, editIcon, openAiIcon, trashIcon } from "../components/ui/icons";
import { useMessage } from "../components/ui/message";
import { PageActions } from "../layouts/PageActions";
import { appStore, useAppStore } from "../store/appStore";

export function CursorSettingsPage() {
  const { providers, models, cursorHarness, cursorBusy } = useAppStore();
  const message = useMessage();
  const [draft, setDraft] = useState<CursorModelDraft | null>(null);
  const [editing, setEditing] = useState<Model | null>(null);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [discovering, setDiscovering] = useState(false);
  const [caCommand, setCaCommand] = useState<string | null>(null);
  const [waitingForCaRefresh, setWaitingForCaRefresh] = useState(false);
  const [deleting, setDeleting] = useState<Model | null>(null);
  const [tabDraft, setTabDraft] = useState<TabSettings | null>(null);
  const [savingTab, setSavingTab] = useState(false);
  const [testingModelHashes, setTestingModelHashes] = useState<Set<string>>(() => new Set());
  const [modelTestResults, setModelTestResults] = useState<Map<string, CursorModelTestState>>(() => new Map());
  const [savingAndTesting, setSavingAndTesting] = useState(false);
  const [batchTesting, setBatchTesting] = useState(false);
  const grouped = useMemo(() => providers.map((provider) => ({ provider, models: models.filter((model) => model.provider_id === provider.provider_id) })).filter((group) => group.models.length > 0), [providers, models]);
  const caReady = cursorHarness?.ca === "ready";
  useEffect(() => {
    if (!caCommand) return;
    void api.copyCursorText(caCommand);
  }, [caCommand]);
  useEffect(() => {
    void api.tabSettings()
      .then(setTabDraft)
      .catch((cause) => message(cause instanceof Error ? cause.message : String(cause)));
  }, [message]);
  const initializeCa = async () => {
    const status = await appStore.initializeCursorCa();
    if (status?.ca === "untrusted" && status.ca_install_command) setCaCommand(status.ca_install_command);
  };

  const openNew = () => { setEditing(null); setModelOptions([]); setDraft(emptyCursorModelDraft()); };
  const openEdit = (model: Model) => {
    const next = emptyCursorModelDraft();
    next.providerMode = String(model.provider_id);
    next.model = {
      model_id: model.model_id, display_name: model.display_name, enabled: model.enabled, sort_order: model.sort_order,
      endpoint_type: model.endpoint_type, request_url: model.request_url,
      context_window_tokens: model.context_window_tokens, max_output_tokens: null,
      reasoning_enabled: model.reasoning_enabled, reasoning_effort: null,
      supports_image_generation: model.supports_image_generation,
    };
    next.modelIds = [model.model_id];
    next.customRequestUrl = Boolean(model.request_url);
    setEditing(model); setModelOptions([model.model_id]); setDraft(next);
  };
  const providerSelection = (value: CursorModelDraft): ProviderSelection => value.providerMode === "new"
    ? { kind: "new", input: { ...value.provider, name: providerName(value.provider.base_url), custom_headers: parseHeaders(value.headersText), extra_params: parseObject(value.extraText, t("额外参数")) } }
    : { kind: "existing", provider_id: Number(value.providerMode) };
  const discover = async () => {
    if (!draft) return;
    setDiscovering(true);
    try {
      const discovered = [...new Set((await api.discoverCursorModels(providerSelection(draft))).models)];
      const existing = draft.providerMode === "new"
        ? new Set<string>()
        : new Set(models.filter((model) => model.provider_id === Number(draft.providerMode)).map((model) => model.model_id));
      setModelOptions(editing ? discovered : discovered.filter((modelId) => !existing.has(modelId)));
    } catch (cause) {
      message(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setDiscovering(false);
    }
  };
  const save = async () => {
    if (!draft) return;
    try {
      const modelInputs = cursorModelInputs(draft, editing !== null);
      const saved = editing
        ? await appStore.updateCursorModel(editing.model_hash, modelInputs[0])
        : await appStore.createCursorModels(providerSelection(draft), modelInputs);
      if (saved) { setDraft(null); setEditing(null); }
    } catch (cause) { message(cause instanceof Error ? cause.message : String(cause)); }
  };
  const testModel = async (model: Model, notify = true) => {
    setTestingModelHashes((current) => new Set(current).add(model.model_hash));
    try {
      const result = await api.testModel(model.model_hash);
      setModelTestResults((current) => new Map(current).set(model.model_hash, { status: "success", result }));
      if (notify) message(t("模型 {model} 连通性测试成功（{duration} ms）", { model: model.display_name, duration: result.duration_ms }));
      return true;
    } catch (cause) {
      const error = cause instanceof Error ? cause.message : String(cause);
      setModelTestResults((current) => new Map(current).set(model.model_hash, { status: "error", error }));
      if (notify) message(t("连通性测试失败：{error}", { error }), { duration: 5000 });
      return false;
    } finally {
      setTestingModelHashes((current) => {
        const next = new Set(current);
        next.delete(model.model_hash);
        return next;
      });
    }
  };
  const testSingleModel = async (model: Model) => {
    await testModel(model);
    await appStore.refresh();
  };
  const saveAndTest = async () => {
    if (!draft || !editing) return;
    setSavingAndTesting(true);
    try {
      const [input] = cursorModelInputs(draft, true);
      const saved = await appStore.updateCursorModel(editing.model_hash, input);
      if (!saved) {
        const error = appStore.getSnapshot().error;
        if (error) message(error);
        return;
      }
      setEditing(saved);
      await testSingleModel(saved);
    } catch (cause) {
      message(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSavingAndTesting(false);
    }
  };
  const testAllModels = async () => {
    if (!models.length || batchTesting) return;
    const targets = [...models];
    setBatchTesting(true);
    try {
      const results = await Promise.all(targets.map((model) => testModel(model, false)));
      await appStore.refresh();
      const successful = results.filter(Boolean).length;
      const failed = targets.length - successful;
      message(failed === 0
        ? t("全部 {count} 个模型连通性测试成功", { count: targets.length })
        : t("连通性测试完成：成功 {successful}，失败 {failed}", { successful, failed }), { duration: failed === 0 ? 2400 : 5000 });
    } finally {
      setBatchTesting(false);
    }
  };
  const list = <div className={styles.groups}>{grouped.map(({ provider, models: childModels }) => <TitledCard key={provider.provider_id} title={<div className={styles.providerTitle}><Icon icon={provider.provider_type === "anthropic" ? claudeIcon : openAiIcon} /><span>{provider.name}</span></div>}>
    <div className={styles.models}>{childModels.map((model) => <div className={styles.modelRow} key={model.model_hash}>
      <div className={styles.modelName}><strong>{model.display_name}</strong><small>{model.model_id} · {model.model_hash}</small></div>
      {/* <span className={styles.badge}>{model.enabled ? t("已启用") : t("已停用")}</span> */}
      {modelTestResults.get(model.model_hash) && <CursorModelTestResult state={modelTestResults.get(model.model_hash)!} />}
      <div className={styles.rowActions}>
        <button type="button" className={`${controls.secondary} ${controls.small}`} disabled={testingModelHashes.size > 0 || cursorBusy || batchTesting} onClick={() => void testSingleModel(model)}>{testingModelHashes.has(model.model_hash) ? t("测试中…") : t("测试")}</button>
        <TooltipTrigger label={t("编辑模型")}><button className={controls.iconButton} aria-label={t("编辑模型")} onClick={() => openEdit(model)}><Icon icon={editIcon} size="1.1em" /></button></TooltipTrigger>
        <TooltipTrigger label={t("删除模型")}><button className={`${controls.iconButton} ${controls.danger}`} aria-label={t("删除模型")} onClick={() => setDeleting(model)}><Icon icon={trashIcon} size="1.1em" /></button></TooltipTrigger>
      </div>
    </div>)}</div>
  </TitledCard>)}</div>;

  const refreshCa = async () => {
    await appStore.refresh();
    if (appStore.getSnapshot().cursorHarness?.ca !== "ready") {
      setWaitingForCaRefresh(false);
    }
  };
  const openCaTerminal = () => {
    if (caCommand) void api.openCursorCaInstallTerminal(caCommand);
    setCaCommand(null);
    setWaitingForCaRefresh(true);
  };
  const saveTab = async () => {
    if (!tabDraft) return;
    try {
      if (tabDraft.mode === "custom" && !tabDraft.address.trim()) throw new Error(t("TAB 服务地址不能为空"));
      setSavingTab(true);
      setTabDraft(await api.setTabSettings(tabDraft));
      message(t("TAB 设置已保存"));
    } catch (cause) {
      message(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSavingTab(false);
    }
  };
  const content = <CursorCaProvider><CursorCaGate busy={cursorBusy} waitingForRefresh={waitingForCaRefresh} onInitialize={() => void initializeCa()} onRefresh={() => void refreshCa()}>
    <div className={styles.page}>
      {tabDraft && <TabSettingsCard settings={tabDraft} saving={savingTab} onChange={setTabDraft} onSave={() => void saveTab()} />}
      <CursorModelProvider><CursorModelGate onAdd={openNew}>{list}</CursorModelGate></CursorModelProvider>
    </div>
  </CursorCaGate></CursorCaProvider>;

  return <>
    {models.length > 0 && <PageActions position="left">
      <button type="button" className={controls.secondary} disabled={cursorBusy || testingModelHashes.size > 0 || batchTesting} onClick={() => void testAllModels()}>{batchTesting ? t("测试中…") : t("一键测试")}</button>
    </PageActions>}
    <PageActions>
      <TooltipTrigger label={caReady ? t("添加模型") : t("请先初始化 CA")}><button className={controls.iconButton} aria-label={t("添加模型")} disabled={!caReady || cursorBusy} onClick={openNew}><Icon icon={addIcon} size="1.1em" /></button></TooltipTrigger>
    </PageActions>
    <PageContent title={t("Cursor 设置")} sections={[{ key: "cursor-settings", estimatedHeight: Math.max(430, models.length * 55 + grouped.length * 62 + 145), content }]} />
    <Modal open={draft !== null} title={editing ? t("编辑模型") : t("添加模型")} busy={cursorBusy || savingAndTesting} onClose={() => setDraft(null)} onSubmit={() => void save()} secondaryAction={editing ? <button type="button" className={controls.secondary} disabled={cursorBusy || savingAndTesting} onClick={() => void saveAndTest()}>{savingAndTesting ? t("测试中…") : t("保存并测试")}</button> : undefined}>
      {draft && <>
        <CursorModelEditor draft={draft} providers={providers} editing={editing !== null} modelOptions={modelOptions} discovering={discovering} onChange={setDraft} onDiscover={() => void discover()} />
        {editing && modelTestResults.get(editing.model_hash) && <div className={styles.editorTestResult}><CursorModelTestResult state={modelTestResults.get(editing.model_hash)!} /></div>}
      </>}
    </Modal>
    <Modal open={caCommand !== null} title={t("安装本地 CA")} closeLabel={t("关闭")} submitLabel={t("打开终端")} onClose={() => setCaCommand(null)} onSubmit={openCaTerminal}>
      <div className={styles.editor}>
        <strong>{t("需要授权安装证书")}</strong>
        <span>{t("安装命令已自动复制。点击“打开终端”，将命令粘贴到终端中执行，并按提示输入密码。")}</span>
        <pre className={styles.command}>{caCommand}</pre>
      </div>
    </Modal>
    <Modal open={deleting !== null} title={t("删除模型")} closeLabel={t("取消")} submitLabel={t("删除")} onClose={() => setDeleting(null)} onSubmit={() => { if (deleting) void appStore.deleteModel(deleting.model_hash); setDeleting(null); }}>
      <p>{t("确定删除这个模型吗？")}</p>
    </Modal>
  </>;
}

function parseObject(text: string, label: string): Record<string, unknown> {
  let parsed: unknown;
  try { parsed = JSON.parse(text || "{}"); } catch { throw new Error(t("{label} 必须是有效 JSON", { label })); }
  if (!parsed || Array.isArray(parsed) || typeof parsed !== "object") throw new Error(t("{label} 必须是 JSON 对象", { label }));
  return parsed as Record<string, unknown>;
}

function cursorModelInputs(draft: CursorModelDraft, editing: boolean) {
  const modelIds = editing
    ? [draft.model.model_id.trim()]
    : [...new Set(draft.modelIds.map((modelId) => modelId.trim()).filter(Boolean))];
  if (!modelIds.length) throw new Error(t("请至少选择或输入一个模型"));
  if (editing && !draft.model.display_name.trim()) throw new Error(t("Model ID 和显示名称不能为空"));
  if (draft.customRequestUrl && !draft.model.request_url.trim()) throw new Error(t("请求完整地址不能为空"));
  if (draft.model.context_window_tokens !== null && (!Number.isSafeInteger(draft.model.context_window_tokens) || draft.model.context_window_tokens <= 0)) throw new Error(t("自定义上下文必须是大于 0 的整数"));
  return modelIds.map((modelId, index) => ({
    ...draft.model,
    model_id: modelId,
    display_name: modelIds.length === 1 ? draft.model.display_name.trim() || modelId : modelId,
    sort_order: draft.model.sort_order + index,
  }));
}

function providerName(baseUrl: string): string {
  try {
    return new URL(baseUrl.trim()).hostname;
  } catch {
    throw new Error(t("Base URL 必须是有效地址"));
  }
}

function parseHeaders(text: string): Record<string, string> {
  const parsed = parseObject(text, t("自定义 Headers"));
  if (Object.values(parsed).some((value) => typeof value !== "string")) throw new Error(t("自定义 Headers 的值必须都是字符串"));
  return parsed as Record<string, string>;
}
