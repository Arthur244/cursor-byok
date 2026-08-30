//! Owns filesystem plugin discovery, sandboxed workers, and plugin providers.
mod asset;
mod builtin;
mod catalog;
mod data;
mod definition;
mod descriptor;
mod installation;
mod manifest;
mod protocol;
mod registry;
mod runtime;
mod state;
mod wire;
mod worker;

pub use descriptor::{
    parse_model_id, PluginDescriptor, PluginModelDescriptor, PluginProviderDescriptor,
    PluginResourceDescriptor, PluginResourceView, ADAPTER_ID_PREFIX,
};
pub use registry::{ImportResponse, OAuthBeginResponse, OAuthPollResponse, PluginRegistry};
pub use runtime::{PluginRuntime, PluginRuntimePhase, PluginRuntimeState, PluginRuntimeStatus};
pub(crate) use wire::llm_request as plugin_llm_request;
