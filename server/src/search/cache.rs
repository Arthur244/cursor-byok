//! Persists fetched web content and serves it from the existing server.
use std::{
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::Router;
use parking_lot::RwLock;
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::{config::managed_data_dir, Error, Result};

const CACHE_ROUTE: &str = "/web-cache";

#[derive(Clone, Default)]
pub struct WebCache {
    inner: Option<Arc<WebCacheInner>>,
}

struct WebCacheInner {
    directory: PathBuf,
    service_addr: RwLock<Option<SocketAddr>>,
}

impl WebCache {
    pub fn managed() -> Result<Self> {
        Self::at(managed_data_dir()?.join("cache/web"))
    }

    pub fn at(directory: PathBuf) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self {
            inner: Some(Arc::new(WebCacheInner {
                directory,
                service_addr: RwLock::new(None),
            })),
        })
    }

    pub fn set_service_addr(&self, address: SocketAddr) {
        if let Some(inner) = &self.inner {
            *inner.service_addr.write() = Some(local_address(address));
        }
    }

    pub async fn store(&self, content: &str) -> Result<Option<String>> {
        let Some(inner) = &self.inner else {
            return Ok(None);
        };
        let address = inner.service_addr.read().ok_or_else(|| {
            Error::Config("web cache is unavailable before the server starts listening".into())
        })?;
        let file_name = format!("{}.txt", Uuid::new_v4());
        let path = inner.directory.join(&file_name);
        let bytes = content.as_bytes().to_vec();
        tokio::task::spawn_blocking(move || fs::write(path, bytes))
            .await
            .map_err(|error| Error::Store(format!("web cache write task failed: {error}")))??;
        Ok(Some(format!("http://{address}{CACHE_ROUTE}/{file_name}")))
    }

    pub fn router(&self) -> Router {
        let Some(inner) = &self.inner else {
            return Router::new();
        };
        Router::new().nest_service(CACHE_ROUTE, ServeDir::new(inner.directory.clone()))
    }

    #[cfg(test)]
    fn directory(&self) -> &Path {
        &self.inner.as_ref().expect("enabled web cache").directory
    }
}

fn local_address(address: SocketAddr) -> SocketAddr {
    match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), address.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), address.port())
        }
        _ => address,
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tempfile::tempdir;
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::WebCache;

    #[tokio::test]
    async fn stores_uuid_named_content_and_serves_it_from_existing_router() {
        let directory = tempdir().unwrap();
        let cache = WebCache::at(directory.path().join("cache/web")).unwrap();
        cache
            .set_service_addr("0.0.0.0:4312".parse().unwrap());

        let location = cache.store("complete fetched content").await.unwrap().unwrap();
        let name = location.rsplit('/').next().unwrap();
        let id = name.strip_suffix(".txt").unwrap();
        assert!(Uuid::parse_str(id).is_ok());
        assert_eq!(
            std::fs::read_to_string(cache.directory().join(name)).unwrap(),
            "complete fetched content"
        );
        assert_eq!(
            location,
            format!("http://127.0.0.1:4312/web-cache/{name}")
        );

        let response = cache
            .router()
            .oneshot(
                Request::get(format!("/web-cache/{name}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap(),
            "complete fetched content"
        );
    }
}
