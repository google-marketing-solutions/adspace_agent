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

//! `dfareporting` v5 REST client for Campaign Manager 360 (port of the
//! `list_cm_*` functions in `actions.py` and of the `placements.patch`,
//! `creatives.patch`, `eventTags.insert|patch|update` and
//! `ads.insert|patch|update` calls used by `helpers.py`).
//!
//! All calls are expressed through the [`Cm360Api`] trait so the
//! reconciliation and execution code can be exercised against a mock; the
//! production implementation is [`Cm360Client`], a thin `reqwest` wrapper that
//! authenticates with a bearer token obtained from a
//! [`SharedTokenProvider`](crate::auth::SharedTokenProvider).

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::time::Duration;

use anyhow::{Context, bail};
use async_trait::async_trait;
use reqwest::header::{ACCEPT, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use serde_json::{Map, Value};
use tracing::{info, warn};

use crate::auth::SharedTokenProvider;

/// Base URL of the `dfareporting` v5 API.
pub const DFAREPORTING_BASE_URL: &str = "https://dfareporting.googleapis.com/dfareporting/v5";

/// OAuth 2.0 scopes required by the CM360 trafficking tools.
///
/// Everything the tools touch (placements, creatives, event tags, ads) is a
/// `dfatrafficking` method, so that is the only scope a session is asked for.
pub const SCOPES: [&str; 1] = ["https://www.googleapis.com/auth/dfatrafficking"];

/// Page size used for paginated list calls.
const MAX_RESULTS: &str = "1000";

/// Upper bound on the number of pages a single list call will follow
/// (`MAX_PAGES × MAX_RESULTS` items). Exceeding it is an error rather than a
/// silently truncated list, because a partial listing would make the
/// reconciliation insert entities that already exist.
pub const MAX_PAGES: usize = 100;

/// Overall request timeout (connect + send + receive).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// TCP/TLS connect timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Retry behaviour for transient CM360 failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first one (`1` disables retries).
    pub max_attempts: u32,
    /// Delay before the first retry; doubles on every further retry.
    pub base_delay: Duration,
    /// Cap on the computed delay (also caps honoured `Retry-After` values).
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }
}

/// True for the response statuses that are worth retrying (quota /
/// rate-limit and transient server errors).
pub fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

/// Parses a `Retry-After` header value: either delay seconds or an HTTP
/// date (RFC 2822 / RFC 7231 format). `None` when absent or unparsable.
pub fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    let value = value?.trim();
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let at = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let delta = at.signed_duration_since(chrono::Utc::now());
    Some(delta.to_std().unwrap_or(Duration::ZERO))
}

/// Delay before retry number `retry` (1-based): the server's `Retry-After`
/// when given, otherwise exponential backoff with "equal" jitter (half of
/// `base_delay × 2^(retry-1)` fixed, half random); both capped at
/// `max_delay`.
pub fn retry_delay(policy: &RetryPolicy, retry: u32, retry_after: Option<Duration>) -> Duration {
    if let Some(after) = retry_after {
        return after.min(policy.max_delay);
    }
    let exponent = retry.saturating_sub(1).min(16);
    let exp = policy
        .base_delay
        .saturating_mul(1u32 << exponent)
        .min(policy.max_delay);
    let half = exp / 2;
    let jitter_fraction = (RandomState::new().build_hasher().finish() % 1_000) as f64 / 1_000.0;
    half + half.mul_f64(jitter_fraction)
}

/// Rejects anything but a non-empty string of ASCII digits — the only shape
/// a CM360 user profile ID interpolated into a URL path may have.
pub fn validate_profile_id(profile_id: &str) -> anyhow::Result<()> {
    if profile_id.is_empty() || !profile_id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("Invalid CM360 profile ID {profile_id:?}: expected a numeric ID.");
    }
    Ok(())
}

/// Builds the shared `reqwest` client with sane timeouts.
fn http_client(timeout: Duration, connect_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(connect_timeout)
        .build()
        .unwrap_or_else(|e| {
            warn!("Falling back to a default reqwest client without timeouts: {e}");
            reqwest::Client::new()
        })
}

/// `fields` mask for `placements.list`.
const PLACEMENT_FIELDS: &str =
    "nextPageToken,placements(id,name,activeStatus,compatibility,pricingSchedule,size,siteId)";
/// `fields` mask for `creatives.list`.
const CREATIVE_FIELDS: &str = "nextPageToken,creatives(id,name,size,type,active)";
/// `fields` mask for `eventTags.list`.
const EVENT_TAG_FIELDS: &str = "eventTags(id,name,url,type,status)";
/// `fields` mask for `ads.list`.
const AD_FIELDS: &str = "nextPageToken,ads(id,name,startTime,endTime,type,deliverySchedule,\
                         creativeRotation,dynamicClickTracker,clickThroughUrl,\
                         placementAssignments,eventTagOverrides)";

/// The subset of the `dfareporting` v5 API used by the trafficking tools.
///
/// Every resource is a raw [`serde_json::Value`] object exactly as returned
/// by the API (or as sent to it).
#[async_trait]
pub trait Cm360Api: Send + Sync {
    /// Lists active placements (all pages), optionally filtered by advertiser
    /// and campaign IDs.
    async fn list_placements(
        &self,
        profile_id: &str,
        advertiser_ids: Option<&[String]>,
        campaign_ids: Option<&[String]>,
    ) -> anyhow::Result<Vec<Value>>;

    /// Lists active, non-archived creatives (all pages), optionally filtered
    /// by advertiser and campaign ID.
    async fn list_creatives(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_id: Option<&str>,
    ) -> anyhow::Result<Vec<Value>>;

    /// Lists event tags, optionally filtered by advertiser and campaign ID.
    async fn list_event_tags(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_id: Option<&str>,
    ) -> anyhow::Result<Vec<Value>>;

    /// Lists non-archived ads (all pages), optionally filtered by advertiser,
    /// campaign and placement IDs.
    async fn list_ads(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_ids: Option<&[String]>,
        placement_ids: Option<&[String]>,
    ) -> anyhow::Result<Vec<Value>>;

    /// `placements.patch`.
    async fn patch_placement(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value>;

    /// `creatives.patch`.
    async fn patch_creative(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value>;

    /// `eventTags.insert`.
    async fn insert_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value>;

    /// `eventTags.patch`.
    async fn patch_event_tag(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value>;

    /// `eventTags.update`.
    async fn update_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value>;

    /// `ads.insert`.
    async fn insert_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value>;

    /// `ads.patch`.
    async fn patch_ad(&self, profile_id: &str, id: &str, body: &Value) -> anyhow::Result<Value>;

    /// `ads.update`.
    async fn update_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value>;
}

/// Production [`Cm360Api`] implementation backed by `reqwest`.
///
/// Requests carry a 60 s overall / 10 s connect timeout, and transient
/// failures (HTTP 429/500/502/503/504 on any method, transport errors on
/// `GET`) are retried with jittered exponential backoff honouring
/// `Retry-After` (see [`RetryPolicy`]).
#[derive(Clone)]
pub struct Cm360Client {
    http: reqwest::Client,
    token_provider: SharedTokenProvider,
    base_url: String,
    retry: RetryPolicy,
}

impl std::fmt::Debug for Cm360Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cm360Client")
            .field("base_url", &self.base_url)
            .field("retry", &self.retry)
            .finish_non_exhaustive()
    }
}

impl Cm360Client {
    /// Creates a client against the public `dfareporting` v5 endpoint.
    pub fn new(token_provider: SharedTokenProvider) -> Self {
        Self::with_base_url(token_provider, DFAREPORTING_BASE_URL)
    }

    /// Creates a client against a custom base URL (e.g. a local test server).
    pub fn with_base_url(token_provider: SharedTokenProvider, base_url: impl Into<String>) -> Self {
        Self {
            http: http_client(REQUEST_TIMEOUT, CONNECT_TIMEOUT),
            token_provider,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            retry: RetryPolicy::default(),
        }
    }

    /// Overrides the retry policy (tests use short delays / fewer attempts).
    pub fn with_retry_policy(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Overrides the request / connect timeouts.
    pub fn with_timeouts(mut self, timeout: Duration, connect_timeout: Duration) -> Self {
        self.http = http_client(timeout, connect_timeout);
        self
    }

    /// Performs one authenticated request against
    /// `{base}/userprofiles/{profile_id}/{resource}` and decodes the JSON
    /// response (an empty body decodes to `{}`), retrying transient failures
    /// per the client's [`RetryPolicy`].
    ///
    /// # Errors
    ///
    /// `"CM360 API error {status} for {METHOD} {url}: {body}"` for non-2xx
    /// responses (after the retries are exhausted for retryable statuses),
    /// an invalid-profile-ID error, plus transport / token / decoding
    /// failures.
    async fn request(
        &self,
        method: Method,
        profile_id: &str,
        resource: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        validate_profile_id(profile_id)?;
        let token = self
            .token_provider
            .access_token()
            .await
            .context("Failed to obtain a CM360 access token")?;
        let url = format!("{}/userprofiles/{profile_id}/{resource}", self.base_url);

        let max_attempts = self.retry.max_attempts.max(1);
        let mut attempt = 1u32;
        loop {
            let mut request = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&token)
                .header(ACCEPT, "application/json")
                .query(query);
            if let Some(body) = body {
                request = request.json(body);
            }

            let response = match request.send().await {
                Ok(response) => response,
                // Only idempotent reads are retried after a transport error:
                // a timed-out POST may well have been applied server-side.
                Err(e) if method == Method::GET && attempt < max_attempts => {
                    let delay = retry_delay(&self.retry, attempt, None);
                    warn!(
                        "CM360 request {method} {url} failed ({e}); retrying in {delay:?} \
                         (attempt {attempt}/{max_attempts})"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
                Err(e) => {
                    return Err(e).with_context(|| format!("CM360 request failed: {method} {url}"));
                }
            };
            let status = response.status();
            let final_url = response.url().to_string();
            let retry_after = parse_retry_after(
                response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
            );
            let text = response.text().await.with_context(|| {
                format!("Failed to read CM360 response body for {method} {final_url}")
            })?;

            if is_retryable_status(status) && attempt < max_attempts {
                let delay = retry_delay(&self.retry, attempt, retry_after);
                warn!(
                    "CM360 API returned {status} for {method} {final_url}; retrying in {delay:?} \
                     (attempt {attempt}/{max_attempts})"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
                continue;
            }
            if !status.is_success() {
                bail!("CM360 API error {status} for {method} {final_url}: {text}");
            }
            if text.trim().is_empty() {
                return Ok(Value::Object(Map::new()));
            }
            return serde_json::from_str(&text).with_context(|| {
                format!("CM360 API returned invalid JSON for {method} {final_url}")
            });
        }
    }

    /// Fetches every page of a list endpoint, following `nextPageToken`, and
    /// concatenates the arrays found under `items_key`.
    ///
    /// # Errors
    ///
    /// Besides request errors, fails when the API hands back a page token it
    /// already served (an endless loop) or when more than [`MAX_PAGES`] pages
    /// would be needed — a truncated listing must never be mistaken for a
    /// complete one.
    async fn list_paginated(
        &self,
        profile_id: &str,
        resource: &str,
        items_key: &str,
        base_query: Vec<(&str, String)>,
    ) -> anyhow::Result<Vec<Value>> {
        let mut items: Vec<Value> = Vec::new();
        let mut page_token: Option<String> = None;
        let mut seen_tokens: Vec<String> = Vec::new();
        for page in 1.. {
            if page > MAX_PAGES {
                bail!(
                    "CM360 {resource} listing exceeded {MAX_PAGES} pages; narrow the request \
                     (advertiser / campaign filters) instead of trafficking against a partial list."
                );
            }
            let mut query = base_query.clone();
            if let Some(token) = &page_token {
                query.push(("pageToken", token.clone()));
            }
            info!("Sending API request to list {resource} (page {page})...");
            let response = self
                .request(Method::GET, profile_id, resource, &query, None)
                .await?;

            if let Some(page_items) = response.get(items_key).and_then(Value::as_array) {
                items.extend(page_items.iter().cloned());
            }
            match response
                .get("nextPageToken")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty())
            {
                Some(token) if seen_tokens.iter().any(|seen| seen == token) => {
                    bail!(
                        "CM360 {resource} listing returned the same nextPageToken {token:?} \
                         twice; aborting to avoid an endless pagination loop."
                    );
                }
                Some(token) => {
                    seen_tokens.push(token.to_string());
                    page_token = Some(token.to_string());
                }
                None => {
                    info!("No more pages (nextPageToken is empty).");
                    break;
                }
            }
        }
        Ok(items)
    }
}

/// Returns the value when it is `Some` and non-empty (Python truthiness).
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

#[async_trait]
impl Cm360Api for Cm360Client {
    async fn list_placements(
        &self,
        profile_id: &str,
        advertiser_ids: Option<&[String]>,
        campaign_ids: Option<&[String]>,
    ) -> anyhow::Result<Vec<Value>> {
        info!(
            "Listing active CM360 placements for Profile={profile_id} (Filters: \
             AdvertiserIds={advertiser_ids:?}, CampaignIds={campaign_ids:?})..."
        );
        let mut query = vec![
            ("maxResults", MAX_RESULTS.to_string()),
            ("activeStatus", "PLACEMENT_STATUS_ACTIVE".to_string()),
            ("fields", PLACEMENT_FIELDS.to_string()),
        ];
        for id in advertiser_ids.unwrap_or_default() {
            query.push(("advertiserIds", id.clone()));
        }
        for id in campaign_ids.unwrap_or_default() {
            query.push(("campaignIds", id.clone()));
        }
        let placements = self
            .list_paginated(profile_id, "placements", "placements", query)
            .await?;
        info!(
            "Successfully fetched {} placements in total.",
            placements.len()
        );
        Ok(placements)
    }

    async fn list_creatives(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_id: Option<&str>,
    ) -> anyhow::Result<Vec<Value>> {
        info!(
            "Listing active CM360 creatives for Profile={profile_id} (Filters: \
             AdvertiserId={advertiser_id:?}, CampaignId={campaign_id:?})..."
        );
        let mut query = vec![
            ("maxResults", MAX_RESULTS.to_string()),
            ("active", "true".to_string()),
            ("archived", "false".to_string()),
            ("fields", CREATIVE_FIELDS.to_string()),
        ];
        if let Some(id) = non_empty(advertiser_id) {
            query.push(("advertiserId", id.to_string()));
        }
        if let Some(id) = non_empty(campaign_id) {
            query.push(("campaignId", id.to_string()));
        }
        let creatives = self
            .list_paginated(profile_id, "creatives", "creatives", query)
            .await?;
        info!(
            "Successfully fetched {} creatives in total.",
            creatives.len()
        );
        Ok(creatives)
    }

    async fn list_event_tags(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_id: Option<&str>,
    ) -> anyhow::Result<Vec<Value>> {
        info!(
            "Listing active CM360 event tags for Profile={profile_id} (Filters: \
             AdvertiserId={advertiser_id:?}, CampaignId={campaign_id:?})..."
        );
        let mut query = vec![("fields", EVENT_TAG_FIELDS.to_string())];
        if let Some(id) = non_empty(advertiser_id) {
            query.push(("advertiserId", id.to_string()));
        }
        if let Some(id) = non_empty(campaign_id) {
            query.push(("campaignId", id.to_string()));
        }
        let response = self
            .request(Method::GET, profile_id, "eventTags", &query, None)
            .await?;
        let event_tags: Vec<Value> = response
            .get("eventTags")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        info!(
            "Successfully fetched {} event tags in total.",
            event_tags.len()
        );
        Ok(event_tags)
    }

    async fn list_ads(
        &self,
        profile_id: &str,
        advertiser_id: Option<&str>,
        campaign_ids: Option<&[String]>,
        placement_ids: Option<&[String]>,
    ) -> anyhow::Result<Vec<Value>> {
        info!(
            "Listing active CM360 ads for Profile={profile_id} (Filters: \
             AdvertiserId={advertiser_id:?}, CampaignIds={campaign_ids:?}, \
             PlacementIds={placement_ids:?})..."
        );
        let mut query = vec![
            ("maxResults", MAX_RESULTS.to_string()),
            ("archived", "false".to_string()),
            ("fields", AD_FIELDS.to_string()),
        ];
        if let Some(id) = non_empty(advertiser_id) {
            query.push(("advertiserId", id.to_string()));
        }
        for id in campaign_ids.unwrap_or_default() {
            query.push(("campaignIds", id.clone()));
        }
        for id in placement_ids.unwrap_or_default() {
            query.push(("placementIds", id.clone()));
        }
        let ads = self.list_paginated(profile_id, "ads", "ads", query).await?;
        info!("Successfully fetched {} ads in total.", ads.len());
        Ok(ads)
    }

    async fn patch_placement(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value> {
        self.request(
            Method::PATCH,
            profile_id,
            "placements",
            &[("id", id.to_string())],
            Some(body),
        )
        .await
    }

    async fn patch_creative(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value> {
        self.request(
            Method::PATCH,
            profile_id,
            "creatives",
            &[("id", id.to_string())],
            Some(body),
        )
        .await
    }

    async fn insert_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(Method::POST, profile_id, "eventTags", &[], Some(body))
            .await
    }

    async fn patch_event_tag(
        &self,
        profile_id: &str,
        id: &str,
        body: &Value,
    ) -> anyhow::Result<Value> {
        self.request(
            Method::PATCH,
            profile_id,
            "eventTags",
            &[("id", id.to_string())],
            Some(body),
        )
        .await
    }

    async fn update_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(Method::PUT, profile_id, "eventTags", &[], Some(body))
            .await
    }

    async fn insert_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(Method::POST, profile_id, "ads", &[], Some(body))
            .await
    }

    async fn patch_ad(&self, profile_id: &str, id: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(
            Method::PATCH,
            profile_id,
            "ads",
            &[("id", id.to_string())],
            Some(body),
        )
        .await
    }

    async fn update_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
        self.request(Method::PUT, profile_id, "ads", &[], Some(body))
            .await
    }
}

/// In-memory [`Cm360Api`] double shared by the `cm360_trafficking` tests.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Mutex;

    use anyhow::anyhow;
    use serde_json::json;

    use super::*;

    /// A recorded list call (`advertiser`, `campaigns`, `placements` hold the
    /// filter values that were passed).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct ListCall {
        pub(crate) method: &'static str,
        pub(crate) profile_id: String,
        pub(crate) advertiser: Vec<String>,
        pub(crate) campaigns: Vec<String>,
        pub(crate) placements: Vec<String>,
    }

    /// A recorded mutation (insert / patch / update) call.
    #[derive(Debug, Clone, PartialEq)]
    pub(crate) struct MutationCall {
        pub(crate) method: &'static str,
        pub(crate) profile_id: String,
        pub(crate) id: Option<String>,
        pub(crate) body: Value,
    }

    /// Produces the response (or error) for a mutation call.
    pub(crate) type Responder = Box<dyn Fn(&MutationCall) -> anyhow::Result<Value> + Send + Sync>;

    /// Produces the error every list call fails with.
    pub(crate) type ListFailure = Box<dyn Fn() -> anyhow::Error + Send + Sync>;

    /// Configurable mock: canned list results, an optional list failure and a
    /// responder closure for mutations. All calls are recorded.
    pub(crate) struct MockApi {
        pub(crate) placements: Vec<Value>,
        pub(crate) creatives: Vec<Value>,
        pub(crate) event_tags: Vec<Value>,
        pub(crate) ads: Vec<Value>,
        list_failure: Option<ListFailure>,
        responder: Responder,
        list_calls: Mutex<Vec<ListCall>>,
        mutation_calls: Mutex<Vec<MutationCall>>,
    }

    impl Default for MockApi {
        fn default() -> Self {
            Self {
                placements: Vec::new(),
                creatives: Vec::new(),
                event_tags: Vec::new(),
                ads: Vec::new(),
                list_failure: None,
                responder: Box::new(|call| {
                    let id = call
                        .id
                        .clone()
                        .or_else(|| {
                            call.body
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| format!("new_{}_id", call.method));
                    Ok(json!({"id": id}))
                }),
                list_calls: Mutex::new(Vec::new()),
                mutation_calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl MockApi {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        pub(crate) fn with_placements(mut self, placements: Vec<Value>) -> Self {
            self.placements = placements;
            self
        }

        pub(crate) fn with_creatives(mut self, creatives: Vec<Value>) -> Self {
            self.creatives = creatives;
            self
        }

        pub(crate) fn with_event_tags(mut self, event_tags: Vec<Value>) -> Self {
            self.event_tags = event_tags;
            self
        }

        pub(crate) fn with_ads(mut self, ads: Vec<Value>) -> Self {
            self.ads = ads;
            self
        }

        pub(crate) fn with_list_error(self, message: &str) -> Self {
            let message = message.to_string();
            self.with_list_failure(move || anyhow!("{message}"))
        }

        /// Makes every list call fail with the error produced by `failure`
        /// (use this to inject typed errors such as `AuthRequired`).
        pub(crate) fn with_list_failure(
            mut self,
            failure: impl Fn() -> anyhow::Error + Send + Sync + 'static,
        ) -> Self {
            self.list_failure = Some(Box::new(failure));
            self
        }

        pub(crate) fn with_responder(
            mut self,
            responder: impl Fn(&MutationCall) -> anyhow::Result<Value> + Send + Sync + 'static,
        ) -> Self {
            self.responder = Box::new(responder);
            self
        }

        pub(crate) fn list_calls(&self) -> Vec<ListCall> {
            self.list_calls.lock().unwrap().clone()
        }

        pub(crate) fn mutation_calls(&self) -> Vec<MutationCall> {
            self.mutation_calls.lock().unwrap().clone()
        }

        fn list(&self, call: ListCall, items: &[Value]) -> anyhow::Result<Vec<Value>> {
            self.list_calls.lock().unwrap().push(call);
            match &self.list_failure {
                Some(failure) => Err(failure()),
                None => Ok(items.to_vec()),
            }
        }

        fn mutate(&self, call: MutationCall) -> anyhow::Result<Value> {
            let result = (self.responder)(&call);
            self.mutation_calls.lock().unwrap().push(call);
            result
        }
    }

    fn strings(values: Option<&[String]>) -> Vec<String> {
        values.map(<[String]>::to_vec).unwrap_or_default()
    }

    fn string(value: Option<&str>) -> Vec<String> {
        value.map(|v| vec![v.to_string()]).unwrap_or_default()
    }

    #[async_trait]
    impl Cm360Api for MockApi {
        async fn list_placements(
            &self,
            profile_id: &str,
            advertiser_ids: Option<&[String]>,
            campaign_ids: Option<&[String]>,
        ) -> anyhow::Result<Vec<Value>> {
            self.list(
                ListCall {
                    method: "list_placements",
                    profile_id: profile_id.to_string(),
                    advertiser: strings(advertiser_ids),
                    campaigns: strings(campaign_ids),
                    placements: Vec::new(),
                },
                &self.placements,
            )
        }

        async fn list_creatives(
            &self,
            profile_id: &str,
            advertiser_id: Option<&str>,
            campaign_id: Option<&str>,
        ) -> anyhow::Result<Vec<Value>> {
            self.list(
                ListCall {
                    method: "list_creatives",
                    profile_id: profile_id.to_string(),
                    advertiser: string(advertiser_id),
                    campaigns: string(campaign_id),
                    placements: Vec::new(),
                },
                &self.creatives,
            )
        }

        async fn list_event_tags(
            &self,
            profile_id: &str,
            advertiser_id: Option<&str>,
            campaign_id: Option<&str>,
        ) -> anyhow::Result<Vec<Value>> {
            self.list(
                ListCall {
                    method: "list_event_tags",
                    profile_id: profile_id.to_string(),
                    advertiser: string(advertiser_id),
                    campaigns: string(campaign_id),
                    placements: Vec::new(),
                },
                &self.event_tags,
            )
        }

        async fn list_ads(
            &self,
            profile_id: &str,
            advertiser_id: Option<&str>,
            campaign_ids: Option<&[String]>,
            placement_ids: Option<&[String]>,
        ) -> anyhow::Result<Vec<Value>> {
            self.list(
                ListCall {
                    method: "list_ads",
                    profile_id: profile_id.to_string(),
                    advertiser: string(advertiser_id),
                    campaigns: strings(campaign_ids),
                    placements: strings(placement_ids),
                },
                &self.ads,
            )
        }

        async fn patch_placement(
            &self,
            profile_id: &str,
            id: &str,
            body: &Value,
        ) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "patch_placement",
                profile_id: profile_id.to_string(),
                id: Some(id.to_string()),
                body: body.clone(),
            })
        }

        async fn patch_creative(
            &self,
            profile_id: &str,
            id: &str,
            body: &Value,
        ) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "patch_creative",
                profile_id: profile_id.to_string(),
                id: Some(id.to_string()),
                body: body.clone(),
            })
        }

        async fn insert_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "insert_event_tag",
                profile_id: profile_id.to_string(),
                id: None,
                body: body.clone(),
            })
        }

        async fn patch_event_tag(
            &self,
            profile_id: &str,
            id: &str,
            body: &Value,
        ) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "patch_event_tag",
                profile_id: profile_id.to_string(),
                id: Some(id.to_string()),
                body: body.clone(),
            })
        }

        async fn update_event_tag(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "update_event_tag",
                profile_id: profile_id.to_string(),
                id: None,
                body: body.clone(),
            })
        }

        async fn insert_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "insert_ad",
                profile_id: profile_id.to_string(),
                id: None,
                body: body.clone(),
            })
        }

        async fn patch_ad(
            &self,
            profile_id: &str,
            id: &str,
            body: &Value,
        ) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "patch_ad",
                profile_id: profile_id.to_string(),
                id: Some(id.to_string()),
                body: body.clone(),
            })
        }

        async fn update_ad(&self, profile_id: &str, body: &Value) -> anyhow::Result<Value> {
            self.mutate(MutationCall {
                method: "update_ad",
                profile_id: profile_id.to_string(),
                id: None,
                body: body.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;
    use crate::auth::StaticTokenProvider;

    /// One HTTP request as seen by the test server.
    #[derive(Debug, Clone)]
    struct Received {
        method: String,
        path: String,
        query: Vec<(String, String)>,
        headers: Vec<(String, String)>,
        body: String,
    }

    impl Received {
        fn header(&self, name: &str) -> Option<&str> {
            self.headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        }

        fn query_values(&self, key: &str) -> Vec<&str> {
            self.query
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .collect()
        }
    }

    async fn read_request(socket: &mut TcpStream) -> Received {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed before headers were complete");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut lines = head.split("\r\n");
        let request_line = lines.next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let headers: Vec<(String, String)> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
            .collect();
        let content_length: usize = headers
            .iter()
            .find(|(k, _)| k == "content-length")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        let mut body = buf[header_end..].to_vec();
        while body.len() < content_length {
            let n = socket.read(&mut chunk).await.unwrap();
            assert!(n > 0, "connection closed before body was complete");
            body.extend_from_slice(&chunk[..n]);
        }
        let parsed = url::Url::parse(&format!("http://test{target}")).unwrap();
        Received {
            method,
            path: parsed.path().to_string(),
            query: parsed
                .query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            headers,
            body: String::from_utf8_lossy(&body).to_string(),
        }
    }

    /// A canned HTTP response of the test server.
    #[derive(Debug, Clone)]
    struct Canned {
        status: u16,
        body: String,
        headers: Vec<(&'static str, String)>,
    }

    fn canned(status: u16, body: &str) -> Canned {
        Canned {
            status,
            body: body.to_string(),
            headers: Vec::new(),
        }
    }

    fn canned_with_header(status: u16, body: &str, header: (&'static str, &str)) -> Canned {
        Canned {
            status,
            body: body.to_string(),
            headers: vec![(header.0, header.1.to_string())],
        }
    }

    /// Spawns a one-request-per-connection HTTP server that answers with the
    /// given `(status, body)` responses in order and records every request.
    async fn spawn_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<Received>>>) {
        spawn_server_with(
            responses
                .into_iter()
                .map(|(status, body)| canned(status, body))
                .collect(),
        )
        .await
    }

    /// [`spawn_server`] with full control over the canned responses.
    async fn spawn_server_with(responses: Vec<Canned>) -> (String, Arc<Mutex<Vec<Received>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&received);
        tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                recorded.lock().unwrap().push(request);
                let reason = match response.status {
                    200 => "OK",
                    403 => "Forbidden",
                    404 => "Not Found",
                    429 => "Too Many Requests",
                    500 => "Internal Server Error",
                    502 => "Bad Gateway",
                    503 => "Service Unavailable",
                    _ => "Unknown",
                };
                let extra: String = response
                    .headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect();
                let body = response.body.as_str();
                let http = format!(
                    "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\n{extra}\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    response.status,
                    body.len()
                );
                socket.write_all(http.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });
        (format!("http://{addr}/"), received)
    }

    fn client(base_url: &str) -> Cm360Client {
        Cm360Client::with_base_url(Arc::new(StaticTokenProvider::new("tok")), base_url)
            .with_retry_policy(fast_retries(4))
    }

    /// A retry policy with negligible delays so tests stay fast.
    fn fast_retries(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
        }
    }

    #[tokio::test]
    async fn list_placements_paginates_and_sends_expected_query() {
        let (base, received) = spawn_server(vec![
            (
                200,
                r#"{"placements":[{"id":"p1","name":"P1"}],"nextPageToken":"tok2"}"#,
            ),
            (
                200,
                r#"{"placements":[{"id":"p2","name":"P2"}],"nextPageToken":""}"#,
            ),
        ])
        .await;
        let api = client(&base);
        let placements = api
            .list_placements(
                "123",
                Some(&["1".to_string()]),
                Some(&["2".to_string(), "3".to_string()]),
            )
            .await
            .unwrap();
        assert_eq!(
            placements,
            vec![
                json!({"id": "p1", "name": "P1"}),
                json!({"id": "p2", "name": "P2"})
            ]
        );

        let requests = received.lock().unwrap().clone();
        assert_eq!(requests.len(), 2);
        let first = &requests[0];
        assert_eq!(first.method, "GET");
        assert_eq!(first.path, "/userprofiles/123/placements");
        assert_eq!(first.query_values("maxResults"), vec!["1000"]);
        assert_eq!(
            first.query_values("activeStatus"),
            vec!["PLACEMENT_STATUS_ACTIVE"]
        );
        assert_eq!(first.query_values("advertiserIds"), vec!["1"]);
        assert_eq!(first.query_values("campaignIds"), vec!["2", "3"]);
        assert_eq!(first.query_values("fields"), vec![PLACEMENT_FIELDS]);
        assert!(first.query_values("pageToken").is_empty());
        assert_eq!(first.header("authorization"), Some("Bearer tok"));
        assert_eq!(first.header("accept"), Some("application/json"));
        assert_eq!(requests[1].query_values("pageToken"), vec!["tok2"]);
    }

    #[tokio::test]
    async fn list_creatives_event_tags_and_ads_send_expected_queries() {
        let (base, received) = spawn_server(vec![
            (200, r#"{"creatives":[{"id":"c1"}]}"#),
            (200, r#"{"eventTags":[{"id":"et1"}]}"#),
            (200, r#"{"ads":[{"id":"a1"}]}"#),
            (200, r#"{}"#),
        ])
        .await;
        let api = client(&base);

        let creatives = api
            .list_creatives("123", Some("1"), Some("2"))
            .await
            .unwrap();
        assert_eq!(creatives.len(), 1);
        let event_tags = api.list_event_tags("123", Some("1"), None).await.unwrap();
        assert_eq!(event_tags.len(), 1);
        let ads = api
            .list_ads(
                "123",
                Some("1"),
                Some(&["2".to_string()]),
                Some(&["3".to_string()]),
            )
            .await
            .unwrap();
        assert_eq!(ads.len(), 1);
        // A response without the items key yields an empty list.
        let none = api.list_ads("123", None, None, None).await.unwrap();
        assert!(none.is_empty());

        let requests = received.lock().unwrap().clone();
        assert_eq!(requests.len(), 4);
        let creatives_req = &requests[0];
        assert_eq!(creatives_req.path, "/userprofiles/123/creatives");
        assert_eq!(creatives_req.query_values("active"), vec!["true"]);
        assert_eq!(creatives_req.query_values("archived"), vec!["false"]);
        assert_eq!(creatives_req.query_values("maxResults"), vec!["1000"]);
        assert_eq!(creatives_req.query_values("advertiserId"), vec!["1"]);
        assert_eq!(creatives_req.query_values("campaignId"), vec!["2"]);
        assert_eq!(creatives_req.query_values("fields"), vec![CREATIVE_FIELDS]);

        let tags_req = &requests[1];
        assert_eq!(tags_req.method, "GET");
        assert_eq!(tags_req.path, "/userprofiles/123/eventTags");
        assert_eq!(tags_req.query_values("advertiserId"), vec!["1"]);
        assert!(tags_req.query_values("campaignId").is_empty());
        assert!(tags_req.query_values("maxResults").is_empty());
        assert_eq!(tags_req.query_values("fields"), vec![EVENT_TAG_FIELDS]);

        let ads_req = &requests[2];
        assert_eq!(ads_req.path, "/userprofiles/123/ads");
        assert_eq!(ads_req.query_values("archived"), vec!["false"]);
        assert_eq!(ads_req.query_values("advertiserId"), vec!["1"]);
        assert_eq!(ads_req.query_values("campaignIds"), vec!["2"]);
        assert_eq!(ads_req.query_values("placementIds"), vec!["3"]);
        assert_eq!(ads_req.query_values("fields"), vec![AD_FIELDS]);
        assert!(requests[3].query_values("advertiserId").is_empty());
    }

    #[tokio::test]
    async fn mutations_use_expected_methods_paths_and_bodies() {
        let (base, received) = spawn_server(vec![
            (200, r#"{"id":"p1"}"#),
            (200, r#"{"id":"c1"}"#),
            (200, r#"{"id":"et_new"}"#),
            (200, r#"{"id":"et1"}"#),
            (200, r#"{"id":"et2"}"#),
            (200, r#"{"id":"ad_new"}"#),
            (200, r#"{"id":"ad1"}"#),
            (200, ""),
        ])
        .await;
        let api = client(&base);
        let body = json!({"name": "X", "active": true});

        assert_eq!(
            api.patch_placement("9", "p1", &body).await.unwrap(),
            json!({"id": "p1"})
        );
        assert_eq!(
            api.patch_creative("9", "c1", &body).await.unwrap(),
            json!({"id": "c1"})
        );
        assert_eq!(
            api.insert_event_tag("9", &body).await.unwrap(),
            json!({"id": "et_new"})
        );
        assert_eq!(
            api.patch_event_tag("9", "et1", &body).await.unwrap(),
            json!({"id": "et1"})
        );
        assert_eq!(
            api.update_event_tag("9", &body).await.unwrap(),
            json!({"id": "et2"})
        );
        assert_eq!(
            api.insert_ad("9", &body).await.unwrap(),
            json!({"id": "ad_new"})
        );
        assert_eq!(
            api.patch_ad("9", "ad1", &body).await.unwrap(),
            json!({"id": "ad1"})
        );
        // Empty bodies decode to an empty object.
        assert_eq!(api.update_ad("9", &body).await.unwrap(), json!({}));

        let requests = received.lock().unwrap().clone();
        let summary: Vec<(String, String, Vec<&str>)> = requests
            .iter()
            .map(|r| (r.method.clone(), r.path.clone(), r.query_values("id")))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "PATCH".to_string(),
                    "/userprofiles/9/placements".to_string(),
                    vec!["p1"]
                ),
                (
                    "PATCH".to_string(),
                    "/userprofiles/9/creatives".to_string(),
                    vec!["c1"]
                ),
                (
                    "POST".to_string(),
                    "/userprofiles/9/eventTags".to_string(),
                    vec![]
                ),
                (
                    "PATCH".to_string(),
                    "/userprofiles/9/eventTags".to_string(),
                    vec!["et1"]
                ),
                (
                    "PUT".to_string(),
                    "/userprofiles/9/eventTags".to_string(),
                    vec![]
                ),
                (
                    "POST".to_string(),
                    "/userprofiles/9/ads".to_string(),
                    vec![]
                ),
                (
                    "PATCH".to_string(),
                    "/userprofiles/9/ads".to_string(),
                    vec!["ad1"]
                ),
                ("PUT".to_string(), "/userprofiles/9/ads".to_string(), vec![]),
            ]
        );
        for request in &requests {
            assert_eq!(serde_json::from_str::<Value>(&request.body).unwrap(), body);
            assert_eq!(request.header("content-type"), Some("application/json"));
            assert_eq!(request.header("authorization"), Some("Bearer tok"));
        }
    }

    #[tokio::test]
    async fn non_2xx_responses_become_errors() {
        let (base, _received) = spawn_server(vec![(403, r#"{"error":"denied"}"#)]).await;
        let api = client(&base);
        let err = api
            .patch_ad("9", "ad1", &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("CM360 API error 403 Forbidden for PATCH http://127.0.0.1:"),
            "{err}"
        );
        assert!(
            err.contains("/userprofiles/9/ads?id=ad1: {\"error\":\"denied\"}"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn constructors_and_debug() {
        let api = Cm360Client::new(Arc::new(StaticTokenProvider::new("tok")));
        assert_eq!(api.base_url, DFAREPORTING_BASE_URL);
        assert_eq!(api.retry, RetryPolicy::default());
        assert_eq!(api.retry.max_attempts, 4);
        let local = client("http://localhost:1/");
        assert_eq!(local.base_url, "http://localhost:1");
        assert!(format!("{local:?}").contains("http://localhost:1"));
        assert_eq!(SCOPES, ["https://www.googleapis.com/auth/dfatrafficking"]);
    }

    #[test]
    fn retry_decision_helpers() {
        for status in [429u16, 500, 502, 503, 504] {
            assert!(
                is_retryable_status(StatusCode::from_u16(status).unwrap()),
                "{status}"
            );
        }
        for status in [200u16, 201, 400, 401, 403, 404, 409, 412, 501] {
            assert!(
                !is_retryable_status(StatusCode::from_u16(status).unwrap()),
                "{status}"
            );
        }

        assert_eq!(parse_retry_after(None), None);
        assert_eq!(parse_retry_after(Some("")), None);
        assert_eq!(parse_retry_after(Some("garbage")), None);
        assert_eq!(parse_retry_after(Some(" 7 ")), Some(Duration::from_secs(7)));
        let future = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc2822();
        let delay = parse_retry_after(Some(&future)).unwrap();
        assert!(delay > Duration::from_secs(25) && delay <= Duration::from_secs(30));
        let past = (chrono::Utc::now() - chrono::Duration::seconds(30)).to_rfc2822();
        assert_eq!(parse_retry_after(Some(&past)), Some(Duration::ZERO));

        let policy = RetryPolicy {
            max_attempts: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(4),
        };
        // Retry-After wins but is capped.
        assert_eq!(
            retry_delay(&policy, 1, Some(Duration::from_secs(2))),
            Duration::from_secs(2)
        );
        assert_eq!(
            retry_delay(&policy, 1, Some(Duration::from_secs(60))),
            Duration::from_secs(4)
        );
        // Exponential backoff with equal jitter: within [exp/2, exp].
        for (retry, exp_ms) in [(1u32, 500u64), (2, 1000), (3, 2000), (4, 4000), (10, 4000)] {
            for _ in 0..20 {
                let delay = retry_delay(&policy, retry, None);
                assert!(
                    delay >= Duration::from_millis(exp_ms / 2)
                        && delay <= Duration::from_millis(exp_ms),
                    "retry {retry}: {delay:?}"
                );
            }
        }
        // Very large retry counts do not overflow.
        assert!(retry_delay(&policy, u32::MAX, None) <= policy.max_delay);
    }

    #[test]
    fn profile_id_validation() {
        assert!(validate_profile_id("7023449").is_ok());
        assert!(validate_profile_id("0").is_ok());
        for bad in ["", " 1", "1 ", "12a", "../admin", "1/2", "١٢٣", "-1"] {
            let err = validate_profile_id(bad).unwrap_err().to_string();
            assert!(err.contains("Invalid CM360 profile ID"), "{bad:?}: {err}");
        }
    }

    #[tokio::test]
    async fn invalid_profile_id_is_rejected_before_any_request() {
        let (base, received) = spawn_server(vec![]).await;
        let api = client(&base);
        let err = api
            .list_placements("../other", None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Invalid CM360 profile ID \"../other\""),
            "{err}"
        );
        let err = api.insert_ad("", &json!({})).await.unwrap_err().to_string();
        assert!(err.contains("Invalid CM360 profile ID"), "{err}");
        assert!(received.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn retries_429_and_5xx_then_succeeds() {
        let (base, received) = spawn_server_with(vec![
            canned_with_header(429, r#"{"error":"rate limited"}"#, ("Retry-After", "0")),
            canned(503, "unavailable"),
            canned(500, ""),
            canned(200, r#"{"id":"ad_new"}"#),
        ])
        .await;
        let api = client(&base);
        let result = api.insert_ad("9", &json!({"name": "A"})).await.unwrap();
        assert_eq!(result, json!({"id": "ad_new"}));
        let requests = received.lock().unwrap().clone();
        assert_eq!(requests.len(), 4);
        for request in &requests {
            assert_eq!(request.method, "POST");
            assert_eq!(request.path, "/userprofiles/9/ads");
            assert_eq!(
                serde_json::from_str::<Value>(&request.body).unwrap(),
                json!({"name": "A"})
            );
        }
    }

    #[tokio::test]
    async fn retries_are_bounded_and_last_error_is_reported() {
        let (base, received) = spawn_server(vec![
            (503, "a"),
            (502, "b"),
            (429, r#"{"error":"still limited"}"#),
        ])
        .await;
        let api = client(&base).with_retry_policy(fast_retries(3));
        let err = api
            .list_ads("9", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.starts_with("CM360 API error 429 Too Many Requests for GET http://127.0.0.1:"),
            "{err}"
        );
        assert!(err.ends_with(r#"{"error":"still limited"}"#), "{err}");
        assert_eq!(received.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn non_retryable_errors_are_not_retried() {
        let (base, received) = spawn_server(vec![(400, r#"{"error":"bad"}"#)]).await;
        let api = client(&base);
        let err = api
            .patch_ad("9", "ad1", &json!({}))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("CM360 API error 400"), "{err}");
        assert_eq!(received.lock().unwrap().len(), 1);

        // A single attempt policy never retries even on 503.
        let (base, received) = spawn_server(vec![(503, "down")]).await;
        let api = client(&base).with_retry_policy(fast_retries(1));
        let err = api
            .list_event_tags("9", None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("CM360 API error 503"), "{err}");
        assert_eq!(received.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn honours_retry_after_header() {
        let (base, _received) = spawn_server_with(vec![
            canned_with_header(429, "", ("Retry-After", "1")),
            canned(200, r#"{"eventTags":[]}"#),
        ])
        .await;
        // Tiny backoff policy; only Retry-After can make this take ~1 s.
        let api = client(&base).with_retry_policy(RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_secs(5),
        });
        let started = std::time::Instant::now();
        let tags = api.list_event_tags("9", None, None).await.unwrap();
        assert!(tags.is_empty());
        assert!(
            started.elapsed() >= Duration::from_millis(900),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn invalid_json_is_an_error() {
        let (base, _received) = spawn_server(vec![(200, "<html>not json</html>")]).await;
        let api = client(&base);
        let err = api.list_ads("9", None, None, None).await.unwrap_err();
        assert!(
            err.to_string()
                .starts_with("CM360 API returned invalid JSON for GET http://127.0.0.1:"),
            "{err}"
        );
        // The serde error is kept as the cause.
        assert_eq!(err.chain().count(), 2);
    }

    #[tokio::test]
    async fn repeated_page_token_aborts_pagination() {
        let (base, received) = spawn_server(vec![
            (200, r#"{"ads":[{"id":"a1"}],"nextPageToken":"t1"}"#),
            (200, r#"{"ads":[{"id":"a2"}],"nextPageToken":"t2"}"#),
            (200, r#"{"ads":[{"id":"a3"}],"nextPageToken":"t1"}"#),
        ])
        .await;
        let api = client(&base);
        let err = api
            .list_ads("9", None, None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("same nextPageToken \"t1\" twice"), "{err}");
        let requests = received.lock().unwrap().clone();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].query_values("pageToken").is_empty());
        assert_eq!(requests[1].query_values("pageToken"), vec!["t1"]);
        assert_eq!(requests[2].query_values("pageToken"), vec!["t2"]);
    }

    #[tokio::test]
    async fn pagination_is_capped() {
        // Every page points to a new token; the client must give up after
        // MAX_PAGES requests instead of looping (the server has one spare
        // response that must never be consumed).
        let responses: Vec<Canned> = (0..=MAX_PAGES)
            .map(|i| {
                canned(
                    200,
                    &format!(r#"{{"creatives":[],"nextPageToken":"t{i}"}}"#),
                )
            })
            .collect();
        let (base, received) = spawn_server_with(responses).await;
        let api = client(&base);
        let err = api
            .list_creatives("9", None, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(&format!("exceeded {MAX_PAGES} pages")),
            "{err}"
        );
        assert_eq!(received.lock().unwrap().len(), MAX_PAGES);
    }

    #[tokio::test]
    async fn get_transport_errors_are_retried_but_posts_are_not() {
        // A listener that accepts but never answers → request timeout.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = Arc::new(Mutex::new(0usize));
        let counter = Arc::clone(&accepted);
        tokio::spawn(async move {
            let mut sockets = Vec::new();
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                *counter.lock().unwrap() += 1;
                sockets.push(socket);
            }
        });
        let api = Cm360Client::with_base_url(
            Arc::new(StaticTokenProvider::new("tok")),
            format!("http://{addr}/"),
        )
        .with_retry_policy(fast_retries(3))
        .with_timeouts(Duration::from_millis(200), Duration::from_millis(200));

        let err = api.list_event_tags("9", None, None).await.unwrap_err();
        assert!(
            err.to_string().starts_with("CM360 request failed: GET"),
            "{err}"
        );
        // The kernel completes the handshake before the server task's
        // `accept()` runs, so give the counter a moment to catch up.
        let accepted_count = |expected: usize| {
            let accepted = Arc::clone(&accepted);
            async move {
                for _ in 0..100 {
                    if *accepted.lock().unwrap() >= expected {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let count = *accepted.lock().unwrap();
                *accepted.lock().unwrap() = 0;
                count
            }
        };
        assert_eq!(accepted_count(3).await, 3);

        let err = api.insert_ad("9", &json!({})).await.unwrap_err();
        assert!(
            err.to_string().starts_with("CM360 request failed: POST"),
            "{err}"
        );
        assert_eq!(accepted_count(1).await, 1);
    }
}
