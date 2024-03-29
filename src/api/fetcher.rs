use crate::reqwest_ext::ResponseExt;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;

/// Fetches url bytes
// Send + sync assumess the implementations to be stateless
// (also: we currently use this only in WASM, which is single threaded)
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait Fetcher: Send + Sync {
    async fn get(&self, url: &str) -> Result<Vec<u8>>;
}

pub struct FetcherImpl {
    client: Client,
}

impl FetcherImpl {
    pub fn new() -> FetcherImpl {
        let client = reqwest::Client::new();
        FetcherImpl { client }
    }
}

impl Default for FetcherImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl Fetcher for FetcherImpl {
    async fn get(&self, url: &str) -> Result<Vec<u8>> {
        Ok(self
            .client
            .get(url)
            .send()
            .await?
            .to_error_if_http_error()
            .await?
            .bytes()
            .await?
            .to_vec())
    }
}
