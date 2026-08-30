//! Exposes plugin runtime initialization and status endpoints.
use axum::{extract::State, Json};

use crate::{plugin::PluginRuntimeStatus, Result};

use super::ControlService;

pub async fn runtime_status(
    State(service): State<ControlService>,
) -> Result<Json<PluginRuntimeStatus>> {
    Ok(Json(service.plugin_runtime_status()))
}

pub async fn initialize_runtime(
    State(service): State<ControlService>,
) -> Result<Json<PluginRuntimeStatus>> {
    Ok(Json(service.initialize_plugin_runtime()))
}

pub async fn cancel_runtime_initialization(
    State(service): State<ControlService>,
) -> Result<Json<PluginRuntimeStatus>> {
    Ok(Json(service.cancel_plugin_runtime_initialization()))
}
