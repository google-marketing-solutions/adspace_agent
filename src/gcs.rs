// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Minimal Google Cloud Storage JSON API client.
//!
//! Only the operations the agent needs: upload/download text objects and
//! list objects under a prefix. Authenticates with any
//! [`AccessTokenProvider`] (normally [`crate::auth::AdcTokenProvider`]).

use anyhow::{Context, bail};
use serde::Deserialize;

use crate::auth::SharedTokenProvider;

/// Base URL of the JSON API.
pub const GCS_API_BASE: &str = "https://storage.googleapis.com/storage/v1";
/// Base URL of the upload endpoint.
pub const GCS_UPLOAD_BASE: &str = "https://storage.googleapis.com/upload/storage/v1";

/// Google Cloud Storage client.
#[derive(Clone)]
pub struct GcsClient {
    http: reqwest::Client,
    token_provider: SharedTokenProvider,
    api_base: String,
    upload_base: String,
}

impl std::fmt::Debug for GcsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcsClient")
            .field("api_base", &self.api_base)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    items: Vec<ObjectItem>,
    #[serde(default)]
    prefixes: Vec<String>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ObjectItem {
    name: String,
}

impl GcsClient {
    /// Creates a client.
    pub fn new(token_provider: SharedTokenProvider) -> Self {
        Self {
            http: reqwest::Client::new(),
            token_provider,
            api_base: GCS_API_BASE.to_string(),
            upload_base: GCS_UPLOAD_BASE.to_string(),
        }
    }

    /// Overrides the API endpoints (tests / emulators).
    pub fn with_endpoints(
        mut self,
        api_base: impl Into<String>,
        upload_base: impl Into<String>,
    ) -> Self {
        self.api_base = api_base.into();
        self.upload_base = upload_base.into();
        self
    }

    fn encode(segment: &str) -> String {
        url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
    }

    /// Uploads `content` to `gs://{bucket}/{object_path}` and returns the URL.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication or the upload fails.
    pub async fn upload_string(
        &self,
        bucket: &str,
        object_path: &str,
        content: &str,
        content_type: &str,
    ) -> anyhow::Result<String> {
        tracing::info!("Uploading payloads to GCS bucket={bucket} path={object_path}...");
        let token = self
            .token_provider
            .access_token()
            .await
            .context("GCS authentication failed")?;
        let url = format!(
            "{}/b/{}/o?uploadType=media&name={}",
            self.upload_base,
            Self::encode(bucket),
            Self::encode(object_path)
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(content.to_string())
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("GCS upload failed ({status}) for gs://{bucket}/{object_path}: {body}");
        }
        let gcs_url = format!("gs://{bucket}/{object_path}");
        tracing::info!("Uploaded successfully to {gcs_url}");
        Ok(gcs_url)
    }

    /// Downloads an object as text.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication or the download fails.
    pub async fn download_string(&self, bucket: &str, object_path: &str) -> anyhow::Result<String> {
        tracing::info!("Downloading payloads from GCS bucket={bucket} path={object_path}...");
        let bytes = self.download_bytes(bucket, object_path).await?;
        String::from_utf8(bytes).context("GCS object is not valid UTF-8")
    }

    /// Downloads an object's raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication or the download fails.
    pub async fn download_bytes(&self, bucket: &str, object_path: &str) -> anyhow::Result<Vec<u8>> {
        let token = self
            .token_provider
            .access_token()
            .await
            .context("GCS authentication failed")?;
        let url = format!(
            "{}/b/{}/o/{}?alt=media",
            self.api_base,
            Self::encode(bucket),
            Self::encode(object_path)
        );
        let resp = self.http.get(&url).bearer_auth(token).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("GCS download failed ({status}) for gs://{bucket}/{object_path}: {body}");
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Lists object names under `prefix`.
    ///
    /// With `delimiter = Some("/")` only direct children are returned and the
    /// second element holds the sub-"directory" prefixes.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication or the listing fails.
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
    ) -> anyhow::Result<(Vec<String>, Vec<String>)> {
        let token = self
            .token_provider
            .access_token()
            .await
            .context("GCS authentication failed")?;
        let mut names = Vec::new();
        let mut prefixes = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut req = self
                .http
                .get(format!("{}/b/{}/o", self.api_base, Self::encode(bucket)))
                .bearer_auth(&token)
                .query(&[("prefix", prefix)]);
            if let Some(d) = delimiter {
                req = req.query(&[("delimiter", d)]);
            }
            if let Some(t) = &page_token {
                req = req.query(&[("pageToken", t.as_str())]);
            }
            let resp = req.send().await?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                bail!("GCS list failed ({status}) for gs://{bucket}/{prefix}: {body}");
            }
            let page: ListResponse = resp.json().await?;
            names.extend(page.items.into_iter().map(|i| i.name));
            prefixes.extend(page.prefixes);
            match page.next_page_token {
                Some(t) if !t.is_empty() => page_token = Some(t),
                _ => break,
            }
        }
        Ok((names, prefixes))
    }
}

/// Splits a `gs://bucket/object/path` URL into `(bucket, object_path)`.
pub fn parse_gs_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("gs://")?;
    let (bucket, object) = rest.split_once('/')?;
    if bucket.is_empty() || object.is_empty() {
        return None;
    }
    Some((bucket, object))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gs_urls() {
        assert_eq!(
            parse_gs_url("gs://bucket/a/b.json"),
            Some(("bucket", "a/b.json"))
        );
        assert_eq!(parse_gs_url("gs://bucket"), None);
        assert_eq!(parse_gs_url("gs://bucket/"), None);
        assert_eq!(parse_gs_url("http://bucket/x"), None);
    }

    #[test]
    fn encodes_object_names() {
        assert_eq!(
            GcsClient::encode("cm360_trafficking/s 1/payloads.json"),
            "cm360_trafficking%2Fs+1%2Fpayloads.json"
        );
    }
}
