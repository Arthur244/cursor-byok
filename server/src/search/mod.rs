//! Exposes provider-independent search capabilities.
mod catalog;
mod engine;
mod federation;
mod fetch;
mod search_provider;

pub use engine::{HtmlEngine, JsonEngine, SearchEngine, SearchHit};
pub use federation::{SearchError, WebSearch};
pub use fetch::{FetchError, FetchedPage, WebFetch};
pub(crate) use search_provider::execute as execute_semble;
