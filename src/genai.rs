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

//! Generative media client for the `google_genai` toolset.
//!
//! Gemini calls (asking about a YouTube video, native image generation) go
//! through adk-rust's own [`adk_gemini::Gemini`] client, which handles the
//! Studio/Vertex backends, credentials and endpoints. Only the two operations
//! the adk-rust client does not expose use REST directly: Imagen `:predict`
//! and Veo `:predictLongRunning` (long-running video generation).
//!
//! Works against Vertex AI (Application Default Credentials) or the Gemini
//! Developer API (API key), mirroring `GOOGLE_GENAI_USE_VERTEXAI`.

use std::sync::Arc;
use std::time::Duration;

use adk_gemini::{
    Content as GeminiContent, FileDataRef, Gemini, GenerationConfig, Message, Part as GeminiPart,
    Role,
};
use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};

use crate::auth::{AdcTokenProvider, SharedTokenProvider};
use crate::config::Settings;

/// Generated media bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedMedia {
    /// MIME type (e.g. `image/png`, `video/mp4`).
    pub mime_type: String,
    /// Raw bytes.
    pub data: Vec<u8>,
}

#[derive(Clone)]
enum Backend {
    Vertex {
        project: String,
        location: String,
        token_provider: SharedTokenProvider,
    },
    Studio {
        api_key: String,
    },
}

/// Client for Gemini / Imagen / Veo.
#[derive(Clone)]
pub struct GenAiClient {
    http: reqwest::Client,
    backend: Backend,
    /// Delay between long-running operation polls.
    poll_interval: Duration,
    /// Maximum time to wait for a long-running operation.
    poll_timeout: Duration,
    base_override: Option<String>,
}

/// Consecutive poll failures tolerated before a Veo operation is given up on.
/// The operation (and its billing) keeps running server-side, so a transient
/// 5xx or network blip must not abort the wait.
const MAX_CONSECUTIVE_POLL_FAILURES: u32 = 5;

/// HTTP client with sane timeouts for the REST-only operations.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default()
}

impl std::fmt::Debug for GenAiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backend = match &self.backend {
            Backend::Vertex {
                project, location, ..
            } => format!("vertex({project}, {location})"),
            Backend::Studio { .. } => "studio".to_string(),
        };
        f.debug_struct("GenAiClient")
            .field("backend", &backend)
            .finish()
    }
}

impl GenAiClient {
    /// Builds a Vertex AI client authenticated with the given token provider
    /// (used for the REST-only operations; Gemini calls use ADC directly).
    pub fn vertex(
        project: impl Into<String>,
        location: impl Into<String>,
        token_provider: SharedTokenProvider,
    ) -> Self {
        Self {
            http: http_client(),
            backend: Backend::Vertex {
                project: project.into(),
                location: location.into(),
                token_provider,
            },
            poll_interval: Duration::from_secs(5),
            poll_timeout: Duration::from_secs(10 * 60),
            base_override: None,
        }
    }

    /// Builds a Gemini Developer API client.
    pub fn studio(api_key: impl Into<String>) -> Self {
        Self {
            http: http_client(),
            backend: Backend::Studio {
                api_key: api_key.into(),
            },
            poll_interval: Duration::from_secs(5),
            poll_timeout: Duration::from_secs(10 * 60),
            base_override: None,
        }
    }

    /// Builds the client from [`Settings`] (Vertex AI with ADC when
    /// `GOOGLE_GENAI_USE_VERTEXAI` is truthy, otherwise `GOOGLE_API_KEY`).
    ///
    /// # Errors
    ///
    /// Returns an error when the required project/location or API key is
    /// missing, or ADC cannot be initialised.
    pub fn from_settings(settings: &Settings) -> anyhow::Result<Self> {
        if settings.use_vertex_ai {
            let project = settings.google_cloud_project.clone().context(
                "GOOGLE_CLOUD_PROJECT must be set when GOOGLE_GENAI_USE_VERTEXAI is true",
            )?;
            let provider: SharedTokenProvider = Arc::new(AdcTokenProvider::new(&[])?);
            Ok(Self::vertex(
                project,
                settings.google_cloud_location.clone(),
                provider,
            ))
        } else {
            let key = settings
                .google_api_key
                .clone()
                .context("GOOGLE_API_KEY must be set when not using Vertex AI")?;
            Ok(Self::studio(key))
        }
    }

    /// Overrides the REST base URL (tests).
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_override = Some(base.into());
        self
    }

    /// Overrides long-running operation polling parameters.
    pub fn with_polling(mut self, interval: Duration, timeout: Duration) -> Self {
        self.poll_interval = interval;
        self.poll_timeout = timeout;
        self
    }

    /// adk-rust Gemini client bound to `model` on the configured backend.
    fn gemini(&self, model: &str) -> anyhow::Result<Gemini> {
        let client = match &self.backend {
            Backend::Vertex {
                project, location, ..
            } => Gemini::with_google_cloud_adc_model(project, location, model.to_string()),
            Backend::Studio { api_key } => Gemini::with_model(api_key, model.to_string()),
        };
        client.map_err(|e| anyhow::anyhow!("failed to build Gemini client for {model}: {e}"))
    }

    fn model_url(&self, model: &str, verb: &str) -> String {
        match &self.backend {
            Backend::Vertex {
                project, location, ..
            } => {
                let base = self.base_override.clone().unwrap_or_else(|| {
                    if location == "global" {
                        "https://aiplatform.googleapis.com".to_string()
                    } else {
                        format!("https://{location}-aiplatform.googleapis.com")
                    }
                });
                format!(
                    "{base}/v1/projects/{project}/locations/{location}/publishers/google/models/{model}:{verb}"
                )
            }
            Backend::Studio { .. } => {
                let base = self
                    .base_override
                    .clone()
                    .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
                format!("{base}/v1beta/models/{model}:{verb}")
            }
        }
    }

    async fn authed(
        &self,
        req: reqwest::RequestBuilder,
    ) -> anyhow::Result<reqwest::RequestBuilder> {
        Ok(match &self.backend {
            Backend::Vertex { token_provider, .. } => {
                req.bearer_auth(token_provider.access_token().await?)
            }
            Backend::Studio { api_key } => req.header("x-goog-api-key", api_key),
        })
    }

    async fn post_json(&self, url: &str, body: &Value) -> anyhow::Result<Value> {
        let req = self.authed(self.http.post(url).json(body)).await?;
        let resp = req
            .send()
            .await
            .with_context(|| format!("request to {url} failed"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("{url} returned HTTP {status}: {text}");
        }
        serde_json::from_str(&text).with_context(|| format!("invalid JSON from {url}"))
    }

    async fn get_json(&self, url: &str) -> anyhow::Result<Value> {
        let req = self.authed(self.http.get(url)).await?;
        let resp = req
            .send()
            .await
            .with_context(|| format!("request to {url} failed"))?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("{url} returned HTTP {status}: {text}");
        }
        serde_json::from_str(&text).with_context(|| format!("invalid JSON from {url}"))
    }

    /// Asks a question about a public YouTube video (Gemini `fileData` part).
    ///
    /// # Errors
    ///
    /// Returns an error when `youtube_video_id` is not a valid video id, or
    /// on transport or API failures.
    pub async fn ask_youtube_video(
        &self,
        model: &str,
        youtube_video_id: &str,
        prompt: &str,
    ) -> anyhow::Result<String> {
        let youtube_video_id = normalize_youtube_video_id(youtube_video_id)
            .with_context(|| format!("'{youtube_video_id}' is not a valid YouTube video id"))?;
        let message = Message {
            role: Role::User,
            content: GeminiContent {
                role: Some(Role::User),
                parts: Some(vec![
                    GeminiPart::FileData {
                        file_data: FileDataRef {
                            mime_type: "video/mp4".to_string(),
                            file_uri: format!("https://www.youtube.com/watch?v={youtube_video_id}"),
                        },
                    },
                    GeminiPart::Text {
                        text: prompt.to_string(),
                        thought: None,
                        thought_signature: None,
                    },
                ]),
            },
        };
        let response = self
            .gemini(model)?
            .generate_content()
            .with_message(message)
            .execute()
            .await
            .map_err(|e| anyhow::anyhow!("Gemini request failed: {e}"))?;
        Ok(response.text())
    }

    /// Generates one image.
    ///
    /// Gemini image models (`gemini-*-image*`) use `generateContent` with image
    /// response modalities through the adk-rust client; any other model is
    /// treated as an Imagen model and called through `:predict`.
    ///
    /// # Errors
    ///
    /// Returns an error when no image is produced or the API fails.
    pub async fn generate_image(
        &self,
        model: &str,
        prompt: &str,
    ) -> anyhow::Result<GeneratedMedia> {
        if model.starts_with("gemini") {
            let config = GenerationConfig {
                response_modalities: Some(vec!["TEXT".to_string(), "IMAGE".to_string()]),
                ..GenerationConfig::default()
            };
            let response = self
                .gemini(model)?
                .generate_content()
                .with_user_message(prompt)
                .with_generation_config(config)
                .execute()
                .await
                .map_err(|e| anyhow::anyhow!("Gemini request failed: {e}"))?;
            return response
                .candidates
                .iter()
                .filter_map(|candidate| candidate.content.parts.as_ref())
                .flatten()
                .find_map(|part| match part {
                    GeminiPart::InlineData { inline_data } => Some(GeneratedMedia {
                        mime_type: inline_data.mime_type.clone(),
                        data: BASE64.decode(&inline_data.data).ok()?,
                    }),
                    _ => None,
                })
                .context("Image generation failed: No images generated.");
        }
        let body =
            json!({ "instances": [{ "prompt": prompt }], "parameters": { "sampleCount": 1 } });
        let response = self
            .post_json(&self.model_url(model, "predict"), &body)
            .await?;
        let prediction = response
            .get("predictions")
            .and_then(Value::as_array)
            .and_then(|p| p.first())
            .context("Image generation failed: No images generated.")?;
        let data = prediction
            .get("bytesBase64Encoded")
            .and_then(Value::as_str)
            .context("Image generation failed: Missing image data.")?;
        let mime_type = prediction
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("image/png")
            .to_string();
        Ok(GeneratedMedia {
            mime_type,
            data: BASE64.decode(data).context("invalid base64 image data")?,
        })
    }

    /// Generates one video with Veo, waiting for the long-running operation.
    ///
    /// # Errors
    ///
    /// Returns an error when the operation fails, times out, or yields no
    /// video bytes.
    pub async fn generate_video(
        &self,
        model: &str,
        prompt: &str,
    ) -> anyhow::Result<GeneratedMedia> {
        let body =
            json!({ "instances": [{ "prompt": prompt }], "parameters": { "sampleCount": 1 } });
        let started = self
            .post_json(&self.model_url(model, "predictLongRunning"), &body)
            .await?;
        let name = started
            .get("name")
            .and_then(Value::as_str)
            .context("Video generation failed: No response.")?
            .to_string();

        let deadline = tokio::time::Instant::now() + self.poll_timeout;
        let mut consecutive_failures: u32 = 0;
        loop {
            let polled = match &self.backend {
                Backend::Vertex { .. } => {
                    self.post_json(
                        &self.model_url(model, "fetchPredictOperation"),
                        &json!({ "operationName": name }),
                    )
                    .await
                }
                Backend::Studio { .. } => {
                    let base = self
                        .base_override
                        .clone()
                        .unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
                    self.get_json(&format!("{base}/v1beta/{name}")).await
                }
            };
            let operation = match polled {
                Ok(operation) => {
                    consecutive_failures = 0;
                    operation
                }
                // Transient failures (5xx, 429, network blips) must not abandon
                // an operation that keeps running - and billing - server-side.
                // Back off and retry a few times before giving up.
                Err(e) => {
                    consecutive_failures += 1;
                    if consecutive_failures >= MAX_CONSECUTIVE_POLL_FAILURES
                        || tokio::time::Instant::now() >= deadline
                    {
                        return Err(e.context(format!(
                            "Video generation: polling operation {name} failed {consecutive_failures} times in a row"
                        )));
                    }
                    tracing::warn!(
                        "Polling video operation {name} failed (attempt {consecutive_failures}): {e:#}"
                    );
                    let backoff = self
                        .poll_interval
                        .saturating_mul(1 << consecutive_failures.min(4));
                    tokio::time::sleep(backoff.min(Duration::from_secs(60))).await;
                    continue;
                }
            };
            if operation
                .get("done")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                if let Some(error) = operation.get("error") {
                    bail!("Video generation failed: {error}");
                }
                let response = operation
                    .get("response")
                    .context("Video generation failed: No response.")?;
                return self.extract_video(response).await;
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "Video generation timed out after {} seconds (operation {name}).",
                    self.poll_timeout.as_secs()
                );
            }
            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn extract_video(&self, response: &Value) -> anyhow::Result<GeneratedMedia> {
        // Vertex AI shape.
        if let Some(video) = response
            .get("videos")
            .and_then(Value::as_array)
            .and_then(|v| v.first())
        {
            let mime_type = video
                .get("mimeType")
                .and_then(Value::as_str)
                .unwrap_or("video/mp4")
                .to_string();
            if let Some(b64) = video.get("bytesBase64Encoded").and_then(Value::as_str) {
                if b64.is_empty() {
                    bail!("Video generation failed: Empty video content.");
                }
                return Ok(GeneratedMedia {
                    mime_type,
                    data: BASE64.decode(b64).context("invalid base64 video data")?,
                });
            }
            if let Some(uri) = video.get("gcsUri").and_then(Value::as_str) {
                bail!(
                    "Video generation stored the result at {uri}; configure the tool without a storage URI to receive bytes."
                );
            }
            bail!("Video generation failed: Missing video data.");
        }
        // Gemini Developer API shape.
        if let Some(sample) = response
            .pointer("/generateVideoResponse/generatedSamples")
            .and_then(Value::as_array)
            .and_then(|s| s.first())
        {
            let uri = sample
                .pointer("/video/uri")
                .and_then(Value::as_str)
                .context("Video generation failed: Missing video data.")?;
            let req = self.authed(self.http.get(uri)).await?;
            let resp = req.send().await?;
            if !resp.status().is_success() {
                bail!("Downloading generated video failed: HTTP {}", resp.status());
            }
            let mime_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("video/mp4")
                .to_string();
            let data = resp.bytes().await?.to_vec();
            if data.is_empty() {
                bail!("Video generation failed: Empty video content.");
            }
            return Ok(GeneratedMedia { mime_type, data });
        }
        if response
            .get("raiMediaFilteredCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        {
            bail!("Video generation failed: the prompt was filtered by responsible-AI policies.");
        }
        bail!("Video generation failed: No videos generated.")
    }
}

static YOUTUBE_VIDEO_ID: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[A-Za-z0-9_-]{11}$").expect("valid regex"));

/// Validates a YouTube video id (eleven URL-safe base64 characters). A full
/// `youtube.com/watch?v=`, `youtu.be/`, `/shorts/`, `/embed/` or `/live/` URL
/// is accepted too and reduced to its id. Anything else yields `None`, so an
/// arbitrary string can never be spliced into the `file_uri` sent to Gemini.
pub fn normalize_youtube_video_id(input: &str) -> Option<String> {
    let input = input.trim();
    if YOUTUBE_VIDEO_ID.is_match(input) {
        return Some(input.to_string());
    }
    let url = url::Url::parse(input).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .trim_start_matches("m.");
    let candidate = match host {
        "youtu.be" => url.path_segments()?.next().map(str::to_string),
        "youtube.com" | "music.youtube.com" | "youtube-nocookie.com" => {
            let mut segments = url.path_segments()?;
            match segments.next() {
                Some("watch") => url
                    .query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned()),
                Some("shorts" | "embed" | "live" | "v") => segments.next().map(str::to_string),
                _ => None,
            }
        }
        _ => None,
    }?;
    YOUTUBE_VIDEO_ID.is_match(&candidate).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::StaticTokenProvider;

    #[test]
    fn youtube_video_ids_are_validated_and_extracted_from_urls() {
        for ok in ["dQw4w9WgXcQ", " dQw4w9WgXcQ ", "a-b_c123XYZ"] {
            assert_eq!(
                normalize_youtube_video_id(ok).as_deref(),
                Some(ok.trim()),
                "{ok:?}"
            );
        }
        for (url, id) in [
            (
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s",
                "dQw4w9WgXcQ",
            ),
            ("https://youtu.be/dQw4w9WgXcQ?si=abc", "dQw4w9WgXcQ"),
            ("https://youtube.com/shorts/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
            ("https://m.youtube.com/embed/dQw4w9WgXcQ", "dQw4w9WgXcQ"),
        ] {
            assert_eq!(
                normalize_youtube_video_id(url).as_deref(),
                Some(id),
                "{url}"
            );
        }
        for bad in [
            "",
            "abc",
            "dQw4w9WgXc",
            "dQw4w9WgXcQQ",
            "dQw4w9WgXc$",
            "abc&list=PL1",
            "x\"></script>",
            "https://evil.example/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=too-short",
            "ftp://youtu.be/dQw4w9WgXcQ",
        ] {
            assert_eq!(
                normalize_youtube_video_id(bad),
                None,
                "{bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn vertex_urls_handle_global_and_regional_locations() {
        let provider: SharedTokenProvider = Arc::new(StaticTokenProvider::new("t"));
        let global = GenAiClient::vertex("p", "global", provider.clone());
        assert_eq!(
            global.model_url("imagen-4.0-generate-001", "predict"),
            "https://aiplatform.googleapis.com/v1/projects/p/locations/global/publishers/google/models/imagen-4.0-generate-001:predict"
        );
        let regional = GenAiClient::vertex("p", "us-central1", provider);
        assert_eq!(
            regional.model_url("veo-3.1-fast-generate-001", "predictLongRunning"),
            "https://us-central1-aiplatform.googleapis.com/v1/projects/p/locations/us-central1/publishers/google/models/veo-3.1-fast-generate-001:predictLongRunning"
        );
        let studio = GenAiClient::studio("key");
        assert_eq!(
            studio.model_url("veo-3.1-fast-generate-001", "predictLongRunning"),
            "https://generativelanguage.googleapis.com/v1beta/models/veo-3.1-fast-generate-001:predictLongRunning"
        );
    }

    #[test]
    fn builds_gemini_clients_for_studio_backend() {
        let client = GenAiClient::studio("key");
        assert!(client.gemini("gemini-3.8-flash").is_ok());
    }
}
