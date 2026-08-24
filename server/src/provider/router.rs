use std::{sync::Arc, time::Duration};

use async_stream::try_stream;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    config::{ProviderConfig, ProviderKind},
    model::{resolve_request_url, ModelInvocation, ModelLatency, NewLlmCall, ProviderType},
    store::Store,
    Error, Result,
};

use super::{
    AnthropicProvider, CallRecorder, OpenAiChatProvider, OpenAiResponsesProvider, Provider,
    ProviderStream,
};

pub struct ProviderRouter {
    store: Store,
    request_timeout: Duration,
}

impl ProviderRouter {
    pub fn new(store: Store, request_timeout: Duration) -> Self {
        Self {
            store,
            request_timeout,
        }
    }
}

impl Provider for ProviderRouter {
    fn stream(
        &self,
        mut invocation: ModelInvocation,
        cancellation: CancellationToken,
    ) -> ProviderStream {
        let store = self.store.clone();
        let request_timeout = self.request_timeout;
        Box::pin(try_stream! {
            let selected = invocation.request.model.model_id.clone();
            let model = store
                .provider_model(&selected)
                .await?
                .filter(|model| model.enabled)
                .ok_or_else(|| Error::Provider(format!("unknown or disabled model: {selected}")))?;
            let endpoint = store
                .provider(model.provider_id)
                .await?
                .ok_or_else(|| Error::Provider(format!("provider {} no longer exists", model.provider_id)))?;
            let request_url = resolve_request_url(
                &endpoint.endpoint.base_url,
                model.endpoint_type,
                &model.request_url,
            )?;
            model.configure(&mut invocation.request.model);
            invocation.request.model.extra_params = endpoint.endpoint.extra_params.clone();
            invocation.request.model.model_id = model.model_id.clone();
            let recorder = CallRecorder::start(store.clone(), NewLlmCall {
                call_id: invocation.call_id.clone(),
                run_id: invocation.run_id.clone(),
                conversation_id: invocation.conversation_id.clone(),
                provider_call_index: invocation.provider_call_index.min(i64::MAX as u64) as i64,
                model_hash: model.model_hash.clone(),
                provider_type: endpoint.endpoint.provider_type,
                provider_url: endpoint.endpoint.base_url.clone(),
                request_type: model.endpoint_type,
                request_url: request_url.clone(),
                model_id: model.model_id.clone(),
                display_name: model.display_name.clone(),
                reasoning_effort: invocation.request.model.reasoning.effort.clone(),
                fast: invocation.request.model.latency == ModelLatency::Fast,
                message_count: invocation.request.history.len(),
                tool_count: invocation.request.prompt.tools.len(),
                detailed: false,
            }).await?;
            let config = ProviderConfig {
                kind: match model.endpoint_type {
                    ProviderType::OpenAiChat => ProviderKind::OpenAiChat,
                    ProviderType::OpenAiResponses => ProviderKind::OpenAiResponses,
                    ProviderType::Anthropic => ProviderKind::Anthropic,
                },
                request_url,
                api_key: endpoint.endpoint.api_key.clone().unwrap_or_default(),
                custom_headers: custom_headers(&endpoint.custom_headers)?,
                max_output_tokens: model.max_output_tokens,
                request_timeout,
            };
            let client = crate::network::client_builder(&store)
                .await?
                .timeout(config.request_timeout)
                .build()?;
            let provider = build_observed(&config, recorder.clone(), client)?;
            let stream_cancellation = cancellation.clone();
            let mut stream = provider.stream(invocation, cancellation);
            while let Some(event) = stream.next().await {
                match event {
                    Ok(event) => {
                        recorder.event(&event).await?;
                        yield event;
                    }
                    Err(error) => {
                        recorder.failed(&error).await?;
                        Err(error)?;
                    }
                }
            }
            if !recorder.is_finished() {
                if stream_cancellation.is_cancelled() {
                    recorder.cancelled().await?;
                } else {
                    let error = Error::Provider("provider stream ended without Done".into());
                    recorder.failed(&error).await?;
                    Err(error)?;
                }
            }
        })
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
    Ok(match config.kind {
        ProviderKind::OpenAiChat => {
            Arc::new(OpenAiChatProvider::new(client, config.clone()).with_recorder(recorder))
        }
        ProviderKind::OpenAiResponses => {
            Arc::new(OpenAiResponsesProvider::new(client, config.clone()).with_recorder(recorder))
        }
        ProviderKind::Anthropic => {
            Arc::new(AnthropicProvider::new(client, config.clone()).with_recorder(recorder))
        }
    })
}
