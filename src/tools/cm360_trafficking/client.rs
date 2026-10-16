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

use anyhow::{Context, bail};
use async_trait::async_trait;
use reqwest::Method;
use reqwest::header::ACCEPT;
use serde_json::{Map, Value};
use tracing::info;

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
#[derive(Clone)]
pub struct Cm360Client {
    http: reqwest::Client,
    token_provider: SharedTokenProvider,
    base_url: String,
}

impl std::fmt::Debug for Cm360Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cm360Client")
            .field("base_url", &self.base_url)
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
            http: reqwest::Client::new(),
            token_provider,
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Performs one authenticated request against
    /// `{base}/userprofiles/{profile_id}/{resource}` and decodes the JSON
    /// response (an empty body decodes to `{}`).
    ///
    /// # Errors
    ///
    /// `"CM360 API error {status} for {METHOD} {url}: {body}"` for non-2xx
    /// responses, plus transport / token / decoding failures.
    async fn request(
        &self,
        method: Method,
        profile_id: &str,
        resource: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<Value> {
        let token = self
            .token_provider
            .access_token()
            .await
            .context("Failed to obtain a CM360 access token")?;
        let url = format!("{}/userprofiles/{profile_id}/{resource}", self.base_url);

        let mut request = self
            .http
            .request(method.clone(), &url)
            .bearer_auth(token)
            .header(ACCEPT, "application/json")
            .query(query);
        if let Some(body) = body {
            request = request.json(body);
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("CM360 request failed: {method} {url}"))?;
        let status = response.status();
        let final_url = response.url().to_string();
        let text = response.text().await.with_context(|| {
            format!("Failed to read CM360 response body for {method} {final_url}")
        })?;

        if !status.is_success() {
            bail!("CM360 API error {status} for {method} {final_url}: {text}");
        }
        if text.trim().is_empty() {
            return Ok(Value::Object(Map::new()));
        }
        serde_json::from_str(&text)
            .with_context(|| format!("CM360 API returned invalid JSON for {method} {final_url}"))
    }

    /// Fetches every page of a list endpoint, following `nextPageToken`, and
    /// concatenates the arrays found under `items_key`.
    async fn list_paginated(
        &self,
        profile_id: &str,
        resource: &str,
        items_key: &str,
        base_query: Vec<(&str, String)>,
    ) -> anyhow::Result<Vec<Value>> {
        let mut items: Vec<Value> = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = base_query.clone();
            if let Some(token) = &page_token {
                query.push(("pageToken", token.clone()));
            }
            info!("Sending API request to list {resource}...");
            let response = self
                .request(Method::GET, profile_id, resource, &query, None)
                .await?;

            if let Some(page) = response.get(items_key).and_then(Value::as_array) {
                items.extend(page.iter().cloned());
            }
            match response
                .get("nextPageToken")
                .and_then(Value::as_str)
                .filter(|token| !token.is_empty())
            {
                Some(token) => page_token = Some(token.to_string()),
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

    /// Configurable mock: canned list results, an optional list failure and a
    /// responder closure for mutations. All calls are recorded.
    pub(crate) struct MockApi {
        pub(crate) placements: Vec<Value>,
        pub(crate) creatives: Vec<Value>,
        pub(crate) event_tags: Vec<Value>,
        pub(crate) ads: Vec<Value>,
        pub(crate) list_error: Option<String>,
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
                list_error: None,
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

        pub(crate) fn with_list_error(mut self, message: &str) -> Self {
            self.list_error = Some(message.to_string());
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
            match &self.list_error {
                Some(message) => Err(anyhow!("{message}")),
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

    /// Spawns a one-request-per-connection HTTP server that answers with the
    /// given `(status, body)` responses in order and records every request.
    async fn spawn_server(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<Received>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&received);
        tokio::spawn(async move {
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                recorded.lock().unwrap().push(request);
                let reason = match status {
                    200 => "OK",
                    403 => "Forbidden",
                    404 => "Not Found",
                    _ => "Unknown",
                };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.ok();
            }
        });
        (format!("http://{addr}/"), received)
    }

    fn client(base_url: &str) -> Cm360Client {
        Cm360Client::with_base_url(Arc::new(StaticTokenProvider::new("tok")), base_url)
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
        let local = client("http://localhost:1/");
        assert_eq!(local.base_url, "http://localhost:1");
        assert!(format!("{local:?}").contains("http://localhost:1"));
        assert_eq!(SCOPES, ["https://www.googleapis.com/auth/dfatrafficking"]);
    }
}
