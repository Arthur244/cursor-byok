use axum::{http::StatusCode, response::IntoResponse, routing::get, Router};
use cursor_server::search::{HtmlEngine, JsonEngine, WebSearch};
use tokio::net::TcpListener;

const RESULT_SELECTOR: &str = ".result";
const TITLE_SELECTOR: &str = ".title";
const LINK_SELECTOR: &str = "a.title";
const SNIPPET_SELECTOR: &str = ".snippet";

#[test]
fn built_in_catalog_covers_the_reference_search_engines() {
    let search = WebSearch::built_in();
    let ids = search.engine_ids();
    for expected in [
        "google",
        "bing",
        "brave",
        "duckduckgo",
        "startpage",
        "yahoo",
        "mojeek",
        "qwant",
        "ecosia",
        "yandex",
        "baidu",
        "sogou",
        "so360",
        "naver",
        "seznam",
        "wikipedia",
        "github",
        "stackoverflow",
        "crates_io",
        "npm",
        "pypi",
        "arxiv",
        "crossref",
    ] {
        assert!(ids.contains(&expected), "missing engine {expected}");
    }
}

#[tokio::test]
async fn federated_search_deduplicates_and_rrf_ranks_results() {
    let base = spawn_search_fixture().await;
    let search = WebSearch::with_engines(vec![
        engine("first", format!("{base}/first?q={{query}}")),
        engine("second", format!("{base}/second?q={{query}}")),
    ]);

    let results = search.search("rust agent").await.unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].url, "https://example.com/shared");
    assert_eq!(results[0].engines, vec!["first", "second"]);
    assert!(results.iter().any(|result| result.title == "Alpha"));
    assert!(results.iter().any(|result| result.title == "Beta"));
}

#[tokio::test]
async fn one_failed_engine_does_not_discard_other_engine_results() {
    let base = spawn_search_fixture().await;
    let search = WebSearch::with_engines(vec![
        engine("failed", format!("{base}/failed?q={{query}}")),
        engine("first", format!("{base}/first?q={{query}}")),
    ]);

    let results = search.search("rust").await.unwrap();

    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn all_failed_engines_return_a_search_error() {
    let base = spawn_search_fixture().await;
    let search =
        WebSearch::with_engines(vec![engine("failed", format!("{base}/failed?q={{query}}"))]);

    let error = search.search("rust").await.unwrap_err();

    assert!(error.to_string().contains("failed"));
}

#[tokio::test]
async fn json_engines_use_declared_result_fields() {
    let base = spawn_search_fixture().await;
    let search = WebSearch::with_engines(vec![JsonEngine::new(
        "json",
        format!("{base}/json?q={{query}}"),
        "/items",
        "/name",
        "/url",
        "/description",
        None,
    )]);

    let results = search.search("rust").await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Structured result");
    assert_eq!(results[0].url, "https://example.com/structured");
    assert_eq!(results[0].chunk, "Parsed from JSON");
}

#[tokio::test]
#[ignore = "live public search smoke test"]
async fn built_in_search_returns_live_results() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("cursor_server::search=debug")
        .try_init();
    let results = WebSearch::built_in()
        .search("Rust programming language")
        .await
        .unwrap();

    assert!(!results.is_empty());
    for result in results {
        println!("{}\t{}\t{:?}", result.title, result.url, result.engines);
    }
}

fn engine(id: &'static str, url: String) -> HtmlEngine {
    HtmlEngine::new(
        id,
        url,
        RESULT_SELECTOR,
        TITLE_SELECTOR,
        LINK_SELECTOR,
        SNIPPET_SELECTOR,
    )
}

async fn spawn_search_fixture() -> String {
    async fn first() -> impl IntoResponse {
        r#"
        <article class="result">
          <a class="title" href="https://example.com/shared?utm_source=one">Shared</a>
          <p class="snippet">Shared from first.</p>
        </article>
        <article class="result">
          <a class="title" href="https://alpha.example/path">Alpha</a>
          <p class="snippet">Alpha result.</p>
        </article>
        "#
    }
    async fn second() -> impl IntoResponse {
        r#"
        <article class="result">
          <a class="title" href="https://example.com/shared#section">Shared result</a>
          <p class="snippet">Shared from second.</p>
        </article>
        <article class="result">
          <a class="title" href="https://beta.example/">Beta</a>
          <p class="snippet">Beta result.</p>
        </article>
        "#
    }
    let app = Router::new()
        .route("/first", get(first))
        .route("/second", get(second))
        .route(
            "/failed",
            get(|| async { (StatusCode::TOO_MANY_REQUESTS, "limited") }),
        )
        .route(
            "/json",
            get(|| async {
                axum::Json(serde_json::json!({
                    "items": [{
                        "name": "Structured result",
                        "url": "https://example.com/structured",
                        "description": "Parsed from JSON"
                    }]
                }))
            }),
        );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}
