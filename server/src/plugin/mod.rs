//! Owns plugin runtime installation and lifecycle infrastructure.
mod asset;
mod installation;
mod runtime;

pub use runtime::{PluginRuntime, PluginRuntimePhase, PluginRuntimeState, PluginRuntimeStatus};
