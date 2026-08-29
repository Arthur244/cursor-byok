//! Interaction query dispatch and approval continuation.

use crate::{
    cursor::{interaction, proto::agent::v1 as pb},
    model::ToolCall,
    search::{WebFetch, WebSearch},
    Error, Result,
};

use super::{normalized, InteractionContinuation, ToolStart};
use crate::cursor::tools::{
    result::{self, ToolResultSender},
    runtime::{CursorToolRuntime, PendingInteraction},
};

pub(super) async fn start(runtime: &CursorToolRuntime, call: &ToolCall) -> Result<ToolStart> {
    let id = runtime.reserve_interaction(call).await?;
    Ok(ToolStart {
        messages: vec![interaction::tool_query(id, call)?],
        completion: None,
    })
}

pub(super) async fn resume(
    results: &ToolResultSender,
    search: &WebSearch,
    fetch: &WebFetch,
    pending: PendingInteraction,
    response: &pb::InteractionResponse,
) -> Result<InteractionContinuation> {
    if normalized(&pending.call.name) == "websearch"
        && matches!(
            response.result.as_ref(),
            Some(pb::interaction_response::Result::WebSearchRequestResponse(
                pb::WebSearchRequestResponse {
                    result: Some(pb::web_search_request_response::Result::Approved(_)),
                }
            ))
        )
    {
        start_web_search(results.clone(), search.clone(), pending)?;
        return Ok(InteractionContinuation::Pending);
    }
    if normalized(&pending.call.name) == "webfetch"
        && matches!(
            response.result.as_ref(),
            Some(pb::interaction_response::Result::WebFetchRequestResponse(
                pb::WebFetchRequestResponse {
                    result: Some(pb::web_fetch_request_response::Result::Approved(_)),
                }
            ))
        )
    {
        start_web_fetch(results.clone(), fetch.clone(), pending)?;
        return Ok(InteractionContinuation::Pending);
    }
    Ok(InteractionContinuation::Completed(Box::new(
        result::from_interaction(pending, response)?,
    )))
}

fn start_web_fetch(
    results: ToolResultSender,
    fetch: WebFetch,
    pending: PendingInteraction,
) -> Result<()> {
    let url = pending
        .call
        .arguments
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| Error::Protocol("WebFetch is missing url".into()))?
        .to_string();
    tokio::spawn(async move {
        let outcome = fetch.fetch(&url).await.map_err(|error| error.to_string());
        match result::complete_web_fetch(pending, outcome) {
            Ok(completion) => results.send(completion),
            Err(error) => results.send_error(error),
        }
    });
    Ok(())
}

fn start_web_search(
    results: ToolResultSender,
    search: WebSearch,
    pending: PendingInteraction,
) -> Result<()> {
    let query = pending
        .call
        .arguments
        .get("search_term")
        .and_then(serde_json::Value::as_str)
        .filter(|query| !query.trim().is_empty())
        .ok_or_else(|| Error::Protocol("WebSearch is missing search_term".into()))?
        .to_string();
    tokio::spawn(async move {
        let outcome = search
            .search(&query)
            .await
            .map_err(|error| error.to_string());
        match result::complete_web_search(pending, outcome) {
            Ok(completion) => results.send(completion),
            Err(error) => results.send_error(error),
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{response::Html, routing::get, Router};
    use serde_json::json;
    use tokio::net::TcpListener;

    use crate::{
        cursor::{proto::agent::v1 as pb, tools::result::tool_result_channel},
        model::ToolCall,
        search::{HtmlEngine, WebFetch, WebSearch},
    };

    use super::{resume, InteractionContinuation, PendingInteraction};

    #[tokio::test]
    async fn approved_web_search_completes_through_the_async_result_channel() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/search",
                    get(|| async {
                        Html(
                            r#"<div class="result"><a class="title" href="https://example.com">Example</a><p class="snippet">Result</p></div>"#,
                        )
                    }),
                ),
            )
            .await
            .unwrap()
        });
        let search = WebSearch::with_engines(vec![HtmlEngine::new(
            "fixture",
            format!("http://{address}/search?q={{query}}"),
            ".result",
            ".title",
            "a.title",
            ".snippet",
        )]);
        let (sender, mut receiver) = tool_result_channel();
        let continuation = resume(
            &sender,
            &search,
            &WebFetch::for_test(),
            pending(),
            &approved(),
        )
        .await
        .unwrap();

        assert!(matches!(continuation, InteractionContinuation::Pending));
        let completion = receiver.recv().await.unwrap().unwrap();
        assert!(!completion.result().is_error);
        assert!(completion.result().content.contains("https://example.com"));
    }

    #[tokio::test]
    async fn approved_web_fetch_completes_without_client_exec() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/article",
                    get(|| async {
                        Html(
                            r#"<html><head><title>Fetched page</title></head><body><article><h1>Fetched page</h1><p>This readable article is long enough for deterministic extraction by the server-side fetch tool.</p><p>It completes directly through the ToolResult channel without creating a Cursor FetchArgs message.</p></article></body></html>"#,
                        )
                    }),
                ),
            )
            .await
            .unwrap()
        });
        let (sender, mut receiver) = tool_result_channel();
        let continuation = resume(
            &sender,
            &WebSearch::built_in(),
            &WebFetch::for_test(),
            pending_fetch(format!("http://{address}/article")),
            &approved_fetch(),
        )
        .await
        .unwrap();

        assert!(matches!(continuation, InteractionContinuation::Pending));
        let completion = receiver.recv().await.unwrap().unwrap();
        assert!(!completion.result().is_error);
        assert!(completion.result().content.contains("Fetched page"));
    }

    fn pending() -> PendingInteraction {
        PendingInteraction {
            call: ToolCall {
                index: 0,
                call_id: "search".into(),
                model_call_id: "model".into(),
                name: "WebSearch".into(),
                arguments_text: r#"{"search_term":"rust"}"#.into(),
                arguments: json!({"search_term": "rust"}),
            },
            started_at_ms: 1,
        }
    }

    fn approved() -> pb::InteractionResponse {
        pb::InteractionResponse {
            id: 1,
            result: Some(pb::interaction_response::Result::WebSearchRequestResponse(
                pb::WebSearchRequestResponse {
                    result: Some(pb::web_search_request_response::Result::Approved(
                        pb::web_search_request_response::Approved::default(),
                    )),
                },
            )),
        }
    }

    fn pending_fetch(url: String) -> PendingInteraction {
        PendingInteraction {
            call: ToolCall {
                index: 0,
                call_id: "fetch".into(),
                model_call_id: "model".into(),
                name: "WebFetch".into(),
                arguments_text: serde_json::to_string(&json!({"url": url})).unwrap(),
                arguments: json!({"url": url}),
            },
            started_at_ms: 1,
        }
    }

    fn approved_fetch() -> pb::InteractionResponse {
        pb::InteractionResponse {
            id: 2,
            result: Some(pb::interaction_response::Result::WebFetchRequestResponse(
                pb::WebFetchRequestResponse {
                    result: Some(pb::web_fetch_request_response::Result::Approved(
                        pb::web_fetch_request_response::Approved::default(),
                    )),
                },
            )),
        }
    }
}
