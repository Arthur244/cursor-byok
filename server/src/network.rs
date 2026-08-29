//! Provides shared network client and transport configuration.
//! Outbound HTTP clients configured from persisted application proxy settings.

use crate::{store::Store, Result};

pub async fn client_builder(store: &Store) -> Result<reqwest::ClientBuilder> {
    let settings = store.proxy_settings_secret().await?;
    // Use the platform TLS stack for compatibility with provider gateways that
    // only offer legacy TLS 1.2 cipher suites unsupported by rustls.
    let mut builder = reqwest::Client::builder().use_native_tls();
    if settings.mode.is_custom() {
        let mut proxy = reqwest::Proxy::all(&settings.address)?;
        if settings.auth_enabled {
            proxy = proxy.basic_auth(&settings.username, &settings.password);
        }
        builder = builder.no_proxy().proxy(proxy);
    }
    Ok(builder)
}

pub async fn client(store: &Store) -> Result<reqwest::Client> {
    Ok(client_builder(store).await?.build()?)
}

pub async fn blocking_client_builder(store: &Store) -> Result<reqwest::blocking::ClientBuilder> {
    let settings = store.proxy_settings_secret().await?;
    let mut builder = reqwest::blocking::Client::builder().use_native_tls();
    if settings.mode.is_custom() {
        let mut proxy = reqwest::Proxy::all(&settings.address)?;
        if settings.auth_enabled {
            proxy = proxy.basic_auth(&settings.username, &settings.password);
        }
        builder = builder.no_proxy().proxy(proxy);
    }
    Ok(builder)
}
