mod catalog;
mod engine;
mod federation;
mod fetch;
mod semble;

pub use engine::{HtmlEngine, JsonEngine, SearchEngine, SearchHit};
pub use federation::{SearchError, WebSearch};
pub use fetch::{FetchError, FetchedPage, WebFetch};
pub(crate) use semble::execute as execute_semble;
