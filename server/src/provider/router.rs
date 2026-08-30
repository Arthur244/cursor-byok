//! Routes model requests to built-in configurations or stable plugin model IDs.
use std::{sync::Arc, time::Duration};

use async_stream::try_stream;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{ProviderConfig, ProviderKind},
    model::{ModelInvocation, ModelLatency, NewLlmCall, ProviderType},
    plugin::{PluginRegistry, ADAPTER_ID_PREFIX},
    store::Store,
    Error, Result,
};

use super::{
    normalize::NormalizedProvider, AnthropicProvider, CallRecorder, OpenAiChatProvider,
    OpenAiResponsesProvider, Provider, ProviderStream,
};

const BUILTIN_PROVIDER_RETRIES: u32 = 5;

pub struct ProviderRouter {
    store: Store,
    plugins: PluginRegistry,
    request_timeout: Duration,
}

impl ProviderRouter {
    pub fn new(store: Store, plugins: PluginRegistry, request_timeout: Duration) -> Self {
        Self {
            store,
            plugins,
            request_timeout,
        }
    }
}

impl Provider for ProviderRouter {
    fn stream(
        &self,
        invocation: ModelInvocation,
        cancellation: CancellationToken,
    ) -> ProviderStream {
        let store = self.store.clone();
        let plugins = self.plugins.clone();
        let request_timeout = self.request_timeout;
        Box::pin(try_stream! {
            let selected = invocation.request.model.model_id.clone();
            if selected.starts_with(ADAPTER_ID_PREFIX) {
                // 插件模型与内置模型走完全相同的流程:Recorder、统一事件、
                // 规范化包装。资源选择与将来的负载均衡都在插件 Provider 内部。
                let plan = plugins.plan_model(&selected).await?;
                let recorder = start_recorder(&store, &invocation, &selected, &plan.model.display_name, ProviderType::Plugin, &plan.request_url, &plan.model.model_id).await?;
                let _cancel_on_drop = recorder.cancel_on_drop();
                recorder.request(serde_json::json!({}), &crate::plugin::plugin_llm_request(&invocation)?).await?;
                let mut routed = invocation.clone();
                routed.request.model.display_name = Some(plan.model.display_name.clone());
                if let Some(tokens) = plan.model.context_window_tokens {
                    routed.request.model.context_window_tokens.get_or_insert(tokens);
                }
                if let Some(tokens) = plan.model.max_output_tokens {
                    routed.request.model.max_output_tokens.get_or_insert(tokens);
                }
                let provider: Arc<dyn Provider> = Arc::new(NormalizedProvider::new(Arc::new(PluginModelProvider {
                    registry: plugins.clone(),
                })));
                let mut stream = provider.stream(routed, cancellation.clone());
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(event) => { recorder.event(&event).await?; yield event; }
                        Err(error) => { recorder.failed(&error).await?; Err(error)?; }
                    }
                }
                finish_stream(&recorder, &cancellation).await?;
            } else {
                let mut routed = invocation.clone();
                let model = store.model(&selected).await?.ok_or_else(|| Error::Provider(format!("unknown model: {selected}")))?;
                let provider_type = model.provider_type();
                let request_url = model.request_url()?;
                model.configure(&mut routed.request.model);
                routed.request.model.extra_params = model.extra_params().clone();
                routed.request.model.model_id = model.model_id.clone();
                let recorder = start_recorder(&store, &invocation, &model.model_hash, &model.display_name, provider_type, &request_url, &model.model_id).await?;
                let _cancel_on_drop = recorder.cancel_on_drop();
                let config = ProviderConfig {
                    kind: provider_kind(provider_type),
                    request_url,
                    api_key: model.api_key.clone(),
                    custom_headers: if model.custom_headers_enabled { custom_headers(&model.custom_headers)? } else { reqwest::header::HeaderMap::new() },
                    max_output_tokens: model.max_output_tokens(),
                    request_timeout,
                    retry_count: BUILTIN_PROVIDER_RETRIES,
                    allowed_body_fields: None,
                };
                let client = crate::network::client_builder(&store).await?.timeout(request_timeout).build()?;
                let provider = build_observed(&config, recorder.clone(), client)?;
                let mut stream = provider.stream(routed, cancellation.clone());
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(event) => { recorder.event(&event).await?; yield event; }
                        Err(error) => { recorder.failed(&error).await?; Err(error)?; }
                    }
                }
                finish_stream(&recorder, &cancellation).await?;
            }
        })
    }
}

async fn start_recorder(
    store: &Store,
    invocation: &ModelInvocation,
    model_hash: &str,
    display_name: &str,
    provider_type: ProviderType,
    request_url: &str,
    model_id: &str,
) -> Result<CallRecorder> {
    CallRecorder::start(
        store.clone(),
        NewLlmCall {
            call_id: invocation.call_id.clone(),
            run_id: invocation.run_id.clone(),
            conversation_id: invocation.conversation_id.clone(),
            provider_call_index: invocation.provider_call_index.min(i64::MAX as u64) as i64,
            model_hash: model_hash.into(),
            provider_type,
            provider_url: request_url.into(),
            request_type: provider_type,
            request_url: request_url.into(),
            model_id: model_id.into(),
            display_name: display_name.into(),
            reasoning_effort: invocation.request.model.reasoning.effort.clone(),
            fast: invocation.request.model.latency == ModelLatency::Fast,
            message_count: invocation.request.history.len(),
            tool_count: invocation.request.prompt.tools.len(),
            detailed: false,
        },
    )
    .await
}

async fn finish_stream(recorder: &CallRecorder, cancellation: &CancellationToken) -> Result<()> {
    if recorder.is_finished() {
        return Ok(());
    }
    if cancellation.is_cancelled() {
        recorder.cancelled().await
    } else {
        let error = Error::Provider("provider stream ended without Done".into());
        recorder.failed(&error).await?;
        Err(error)
    }
}

/// 插件模型的 Provider 实现;对路由与规范化层完全等同于内置 Provider。
struct PluginModelProvider {
    registry: PluginRegistry,
}

impl Provider for PluginModelProvider {
    fn stream(
        &self,
        invocation: ModelInvocation,
        cancellation: CancellationToken,
    ) -> ProviderStream {
        self.registry.stream_model(invocation, cancellation)
    }
}

fn provider_kind(provider_type: ProviderType) -> ProviderKind {
    match provider_type {
        ProviderType::OpenAiChat => ProviderKind::OpenAiChat,
        ProviderType::OpenAiResponses => ProviderKind::OpenAiResponses,
        ProviderType::Anthropic => ProviderKind::Anthropic,
        // 内置模型的 provider_type 只来自 ModelType,不可能是插件。
        ProviderType::Plugin => unreachable!("plugin models never use built-in provider configs"),
    }
}

fn custom_headers(value: &serde_json::Value) -> Result<reqwest::header::HeaderMap> {
    let object = value
        .as_object()
        .ok_or_else(|| Error::Config("custom headers must be an object".into()))?;
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in object {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| Error::Config(format!("invalid custom header name: {error}")))?;
        let value = value
            .as_str()
            .ok_or_else(|| Error::Config("custom header values must be strings".into()))?;
        let value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|error| Error::Config(format!("invalid custom header value: {error}")))?;
        headers.insert(name, value);
    }
    Ok(headers)
}

pub fn build(config: &ProviderConfig) -> Result<Arc<dyn Provider>> {
    build_inner(config, None, None)
}

fn build_observed(
    config: &ProviderConfig,
    recorder: CallRecorder,
    client: reqwest::Client,
) -> Result<Arc<dyn Provider>> {
    build_inner(config, Some(recorder), Some(client))
}

fn build_inner(
    config: &ProviderConfig,
    recorder: Option<CallRecorder>,
    client: Option<reqwest::Client>,
) -> Result<Arc<dyn Provider>> {
    let client = match client {
        Some(client) => client,
        None => reqwest::Client::builder()
            .timeout(config.request_timeout)
            .build()?,
    };
    let provider: Arc<dyn Provider> = match config.kind {
        ProviderKind::OpenAiChat => {
            Arc::new(OpenAiChatProvider::new(client, config.clone()).with_recorder(recorder))
        }
        ProviderKind::OpenAiResponses => {
            Arc::new(OpenAiResponsesProvider::new(client, config.clone()).with_recorder(recorder))
        }
        ProviderKind::Anthropic => {
            Arc::new(AnthropicProvider::new(client, config.clone()).with_recorder(recorder))
        }
    };
    Ok(Arc::new(NormalizedProvider::new(provider)))
}
