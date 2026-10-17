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

//! ADK tools for parsing CM360 trafficking sheets and executing trafficking.
//!
//! The workflow guidance that used to live in a separate `cm360-trafficking`
//! skill is embedded here: the toolset injects it into every model request
//! through [`Toolset::process_llm_request`], and the hard rules are repeated
//! in the tool descriptions, so there is a single source of truth for the
//! CM360 trafficking behaviour.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use adk_rust::{Content, LlmRequest, Part, ReadonlyContext, Result, Tool, ToolContext, Toolset};
use async_trait::async_trait;
use serde_json::{Value, json};

use super::client::{Cm360Api, Cm360Client, SCOPES};
use super::diff::{resolve_and_build_operations, resolve_creative_ids, resolve_placement_ids};
use super::execute::{
    build_trafficking_summary_response, entity_outcomes, partition_operations,
    process_ad_operations, process_creative_operations, process_event_tag_operations,
    process_placement_operations,
};
use super::grouping::{group_ads, group_creatives, group_event_tags, group_placements};
use super::sheet::{RawSheet, apply_trafficked_status, parse_csv, prepare_data, to_csv};
use super::validations::validate_sheet;
use crate::auth::{AuthRequired, SessionCredentialManager, SessionTokenProvider, auth_required};

/// Session-state key holding the artifact name of the staged payload.
///
/// Set by `parse_sheet_tool`; cleared (set to `null`) by
/// `traffic_campaigns_in_cm360_tool` once the payload has been executed, so
/// a staged payload can only ever be pushed once.
pub const PARSED_PAYLOAD_ARTIFACT_KEY: &str = "cm360_parsed_payload_artifact";
/// Session-state key holding the [`PAYLOAD_ID_FIELD`] of the payload that was
/// last executed (second line of defence against re-running a payload).
pub const EXECUTED_PAYLOAD_ID_KEY: &str = "cm360_executed_payload_id";
/// Artifact (session-scoped, versioned) holding the staged CM360 operations.
pub const PAYLOAD_ARTIFACT_NAME: &str = "cm360_payloads.json";
/// Field of the staged payload holding its unique ID.
pub const PAYLOAD_ID_FIELD: &str = "payload_id";
/// Field of the staged payload naming the CSV artifact it was parsed from.
pub const PAYLOAD_SOURCE_CSV_FIELD: &str = "source_csv";

/// Workflow instructions injected into every model request while the CM360
/// trafficking toolset is enabled (formerly the `cm360-trafficking` skill).
pub const CM360_TRAFFICKING_INSTRUCTIONS: &str = r#"# CM360 Campaign Trafficking Instructions

Apply these instructions ONLY when the user asks about trafficking, pushing, or editing campaigns in Campaign Manager 360 (CM360) (for example, "push my campaigns to CM360", "edit my campaigns", "traffic in CM360", or "parse trafficking sheet"). Do NOT apply them outside of CM360 campaign trafficking contexts.

## 1. Greeting & File Upload

- Greet the user:
  > "Hi! I am your CM360 Campaign Trafficking Agent. I can help you set up your campaigns in Campaign Manager 360. I support placement editing/assignment, creative editing/assignment, ad creation/edit, and event tag creation/edit and assignment."
- Ask the user to upload their trafficking `.csv` file (if not already provided) and acknowledge receipt.

## 2. Parse the Trafficking Sheet (`parse_sheet_tool`)

- Call **`parse_sheet_tool`** first to validate and parse the `.csv` artifact.
- **Strict Tool Restriction**: Never use code execution or data analysis tools (`gemini_code_execution`, `python_code`, `vertex_sandbox_code_execution`, ...) to read or parse the CSV. Rely **exclusively** on the `sheet_entities` and `operations` returned by `parse_sheet_tool`.
- If `parse_sheet_tool` returns `status: "AUTH_REQUIRED"`, show the user the authorization link, ask them to complete it, and call the tool again afterwards.
- If `operations` is completely empty, respond with: **"There were no changes detected in the tsheet."** and do not request approval.
- **Do NOT** call `traffic_campaigns_in_cm360_tool` without explicit user approval.

## 3. Present the Parsed Sheet Summary & Request Approval

Build four Markdown tables (**Placements**, **Ads**, **Creatives**, and **Event Tags**) from `sheet_entities` (`placements`, `ads`, `creatives`, `event_tags`) and `operations`:

- **`insert` operation**: Operation = `Create`, Changes / Diffs = `N/A (New {entity})`.
- **`patch` operation**: Operation = `Edit`, Changes / Diffs = summarize modified fields from `diff_fields` (never omit diffs; specify whether Click-Through URL changes are at the ad or creative level).
- **Not in `operations`**: Operation = `None`, Changes / Diffs = `None`.

Use this template:

```markdown
Based on the trafficking sheet provided, here are the details for the entities in CM360:

### Placements
| Placement Name | Site | Size | Type | Start Date | End Date | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- | --- | --- |
| TestPlacement1 | Test Site 1 | 2920x1796 | Display | 9/7/2026 | 9/30/2026 | Edit | Flight dates (6/10/26-7/10/26 → 9/7/26-9/30/26) |
| TestPlacement2 | Test Site 2 | 2920x1796 | Display | 6/10/2026 | 7/10/2026 | None | None |

*Note for Placement Date Updates (include whenever placement dates are edited): When updating placement flight dates in CM360, the placement's pricing schedule periods (`pricingPeriods`) are automatically synchronized to match the new placement date range.*

### Ads
| Ad Name | Ad Type | Start Date | End Date | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- |
| Test Ad C | AD_SERVING_STANDARD_AD | 9/7/2026 | 9/30/2026 | Create | N/A (New Ad) |
| Test Ad D | AD_SERVING_STANDARD_AD | 9/7/2026 | 9/30/2026 | Edit | Flight dates, Placement assignments |

### Creatives
| Creative Name | Type | Dimensions | Rotation | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- | --- |
| Test Creative 1 | HTML5 | 2920x1796 | 100% | None | None |
| Test Creative 2 (copy) | HTML5 | 2920x1796 | 33% | None | None |

ℹ️ *Notes on Creatives:*
1. *If only creative rotations were updated in the sheet, creatives show `None` here because creative assets (such as dimensions) were not modified. In CM360, creative rotation is managed at the Ad level, so rotation updates are applied to the corresponding **Ads** above.*
2. *If an ad has only one creative in rotation, weight changes in the sheet do not take effect since CM360 does not apply weights.*

### Event Tags
| Event Tag Name | Type | URL | Operation | Changes / Diffs |
| --- | --- | --- | --- | --- |
| Test_Impression_Tag | IMPRESSION_JAVASCRIPT_EVENT_TAG | https://example.com/imp | Create | N/A (New Tag) |
| Test_Click_Tag | CLICK_THROUGH_EVENT_TAG | https://example.com/click | Edit | URL (updated) |

*Note: Placements and Creatives are existing CM360 entities that will be assigned or edited. Ads and Event Tags will be created or edited based on their operations. Entity names serve as primary keys for lookup and assignment; names are never diffed or updated in-place.*

**Do I have your approval to proceed with creating and editing these entities in Campaign Manager 360?**
```

## 4. Execute Trafficking (`traffic_campaigns_in_cm360_tool`) & Present Results

- After the user gives explicit approval, call **`traffic_campaigns_in_cm360_tool`** exactly once per parsed payload. A staged payload is consumed by the call: to push anything again (for example after a `PARTIAL_SUCCESS` / `ERROR` result, or because the user edited the CSV) you MUST call `parse_sheet_tool` again, present the new summary, and obtain approval again. Rows whose entities failed keep their `New` / `Update` status in the CSV artifact, so the next parse only re-stages what still needs to be trafficked.
- Present the execution results for **Placements**, **Ads**, **Creatives**, and **Event Tags** (indicate `None` if a section had no executed operations, and note when creative rotations were applied at the Ad level). Mention any `ERROR` rows with their messages, and if `trafficking_sheet.status` is `ERROR` tell the user that the CSV artifact could not be updated. Then give the instructions to download the updated `Trafficked` CSV artifact:

```markdown
Campaign Manager 360 trafficking execution completed successfully for campaign **{campaign_name}** (ID: `{ID}`, Advertiser ID: `{advertiser_id}`, Profile ID: `{profile_id}`).

### Placements
| Placement Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| TestPlacement1 | Edit | 12345678 | SUCCESS |

### Ads
| Ad Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| Test Ad C | Create | 99887766 | SUCCESS |
| Test Ad D | Edit | 99887767 | SUCCESS |

### Creatives
| Creative Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| Creative Test 1 | Edit | 55667788 | SUCCESS |

### Event Tags
| Event Tag Name | Operation | CM360 ID | Status |
| --- | --- | --- | --- |
| event1 | Create | 11223344 | SUCCESS |
| event2 | Create | 11223345 | SUCCESS |

---

### Updated Trafficking Sheet Artifact
The trafficking sheet artifact has been updated in your session with the **`Trafficked`** status for all successfully created/edited entities.

To download the updated trafficking sheet:
- **Step 1**: Open the **Artifacts** panel on the right sidebar of the Web UI.
- **Step 2**: Select the trafficking CSV file from the artifacts list.
- **Step 3**: Click **Download** in the artifact preview window to save the updated CSV.
```
"#;

/// Shared dependencies of the CM360 tools.
#[derive(Clone)]
pub struct Cm360Services {
    /// Session-bound Google OAuth credentials.
    pub credentials: Arc<SessionCredentialManager>,
    /// Optional override of the CM360 API (tests).
    pub api_override: Option<Arc<dyn Cm360Api>>,
}

impl std::fmt::Debug for Cm360Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cm360Services").finish_non_exhaustive()
    }
}

impl Cm360Services {
    /// CM360 OAuth scopes as owned strings.
    pub fn scopes() -> Vec<String> {
        SCOPES.iter().map(|s| s.to_string()).collect()
    }

    fn api_for(&self, ctx: &Arc<dyn ToolContext>) -> Arc<dyn Cm360Api> {
        if let Some(api) = &self.api_override {
            return api.clone();
        }
        let provider =
            SessionTokenProvider::new(self.credentials.clone(), ctx.clone(), Self::scopes());
        Arc::new(Cm360Client::new(Arc::new(provider)))
    }

    /// Ensures the session has valid CM360 credentials before any API call.
    async fn ensure_authorized(
        &self,
        ctx: &Arc<dyn ToolContext>,
    ) -> std::result::Result<(), AuthRequired> {
        if self.api_override.is_some() {
            return Ok(());
        }
        match self
            .credentials
            .access_token(ctx.as_ref(), &Self::scopes())
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => match auth_required(&e) {
                Some(required) => Err(required.clone()),
                None => Err(AuthRequired {
                    auth_url: String::new(),
                    scopes: Vec::new(),
                    message: format!("Google authorization failed: {e:#}"),
                }),
            },
        }
    }
}

fn error_json(tool: &str, message: impl std::fmt::Display) -> Value {
    json!({ "status": "ERROR", "message": format!("[{tool}] - {message}") })
}

fn pretty(value: &Value) -> Value {
    // Python returned `json.dumps(..., indent=2)` strings; returning the JSON
    // object itself is friendlier to the model and the UI.
    value.clone()
}

/// Name of the latest CSV artifact of the session.
async fn latest_csv_name(ctx: &Arc<dyn ToolContext>) -> anyhow::Result<String> {
    let artifacts = ctx
        .artifacts()
        .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
    let files = artifacts.list().await?;
    tracing::info!("📂 Discovered artifacts in session: {files:?}");
    files
        .iter()
        .rfind(|f| f.to_lowercase().ends_with(".csv"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No CSV trafficking files (.csv) found in session artifacts. Please upload your trafficking CSV file."))
}

/// Loads the named CSV artifact of the session as a raw sheet.
async fn load_raw_sheet(ctx: &Arc<dyn ToolContext>, name: &str) -> anyhow::Result<RawSheet> {
    let artifacts = ctx
        .artifacts()
        .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
    let part = artifacts
        .load(name)
        .await
        .map_err(|e| anyhow::anyhow!("Artifact '{name}' could not be loaded: {e}"))?;
    let text = match part {
        Part::Text { text } => text,
        Part::InlineData { data, .. } => String::from_utf8_lossy(&data).to_string(),
        _ => anyhow::bail!("Failed to extract data from artifact '{name}'."),
    };
    parse_csv(&text)
}

fn state_get(ctx: &Arc<dyn ToolContext>, key: &str) -> Option<Value> {
    ctx.session().and_then(|s| s.state().get(key))
}

/// Non-empty string value of a session-state key.
fn state_get_str(ctx: &Arc<dyn ToolContext>, key: &str) -> Option<String> {
    state_get(ctx, key)
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|s| !s.is_empty())
}

/// Parses a CM360 trafficking sheet and stages the resolved operations.
pub struct ParseSheetTool {
    services: Cm360Services,
}

impl ParseSheetTool {
    /// Creates the tool.
    pub fn new(services: Cm360Services) -> Self {
        Self { services }
    }
}

#[async_trait]
impl Tool for ParseSheetTool {
    fn name(&self) -> &str {
        "parse_sheet_tool"
    }

    fn description(&self) -> &str {
        "Parses the Campaign Manager 360 (CM360) trafficking sheet (the latest CSV artifact uploaded in \
         this session), validates all required fields, groups placements, creatives, event tags and ads, \
         resolves existing CM360 placement/creative IDs, generates the create/update operations and \
         stages them for execution. Always call this BEFORE traffic_campaigns_in_cm360_tool. Never parse \
         the CSV with code execution or other tools; rely exclusively on this tool's `sheet_entities` \
         and `operations`. Returns status AUTH_REQUIRED with an authorization URL when the user must \
         first grant CM360 access."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
        tracing::info!("🚀 Starting parse_sheet_tool...");
        if let Err(required) = self.services.ensure_authorized(&ctx).await {
            return Ok(required.to_tool_response());
        }
        if ctx.session_id().is_empty() {
            return Ok(error_json(
                "parse_sheet_tool",
                "Session ID is required and must be present in the tool context.",
            ));
        }
        match self.run(&ctx).await {
            Ok(value) => Ok(value),
            Err(e) => {
                if let Some(required) = auth_required(&e) {
                    return Ok(required.to_tool_response());
                }
                tracing::error!(
                    "❌ [parse_sheet_tool] - Error parsing the tsheet to generate payloads: {e:#}"
                );
                Ok(error_json(
                    "parse_sheet_tool",
                    format!("Error parsing the tsheet to generate payloads: {e:#}"),
                ))
            }
        }
    }
}

impl ParseSheetTool {
    async fn run(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<Value> {
        // 1. Load the raw sheet. The CSV is chosen here, once, and recorded
        //    in the staged payload so the execution step updates the very
        //    file the operations were derived from.
        let source_csv = latest_csv_name(ctx).await?;
        tracing::info!("🎯 Selecting target campaign file: {source_csv}");
        let raw = load_raw_sheet(ctx, &source_csv).await?;

        // 2. Extract metadata and clean data rows.
        let (rows, meta) = prepare_data(&raw)?;
        tracing::info!(
            "🆔 [parse_sheet_tool] - Extracted IDs: Profile={}, Advertiser={}, Campaign={}, Campaign Name={:?}",
            meta.profile_id,
            meta.advertiser_id,
            meta.campaign_id,
            meta.campaign_name
        );

        // 3. Run validations.
        let validation_errors = validate_sheet(
            &rows,
            Some(&meta.profile_id),
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
        );
        if !validation_errors.is_empty() {
            tracing::info!("❌ [parse_sheet_tool] - Validation failed: {validation_errors:?}");
            return Ok(json!({
                "status": "error",
                "message": "[parse_sheet_tool] - Validation failed for the trafficking sheet.",
                "validation_errors": validation_errors,
            }));
        }

        // 4. Group entities.
        let advertiser_id = Some(meta.advertiser_id.as_str());
        let campaign_id = Some(meta.campaign_id.as_str());
        let campaign_name = meta.campaign_name.as_deref();
        let placements = group_placements(&rows, advertiser_id, campaign_id, campaign_name);
        let creatives = group_creatives(&rows, advertiser_id);
        let event_tags = group_event_tags(&rows, advertiser_id, campaign_id);
        let mut ads = group_ads(&rows, advertiser_id, campaign_id, campaign_name);

        // 5. Resolve existing placements and creatives.
        let api = self.services.api_for(ctx);
        let existing_placements = resolve_placement_ids(
            api.as_ref(),
            &mut ads,
            &meta.profile_id,
            advertiser_id,
            campaign_id,
        )
        .await?;
        let existing_creatives = resolve_creative_ids(
            api.as_ref(),
            &mut ads,
            &meta.profile_id,
            advertiser_id,
            campaign_id,
        )
        .await?;
        tracing::info!(
            "📊 [parse_sheet_tool] - Grouped: {} placements, {} creatives, {} event tags, {} ads.",
            placements.len(),
            creatives.len(),
            event_tags.len(),
            ads.len()
        );

        // 6. Reconcile against live event tags / ads and build operations.
        let operations = resolve_and_build_operations(
            api.as_ref(),
            &mut ads,
            &event_tags,
            &meta.profile_id,
            advertiser_id,
            campaign_id,
            Some(&placements),
            Some(&creatives),
            Some(&existing_placements),
            Some(&existing_creatives),
        )
        .await?;
        tracing::info!(
            "✨ [parse_sheet_tool] - Successfully generated {} operations.",
            operations.len()
        );

        // 7. Stage the parsed payload as a session artifact (versioned; the
        //    artifact service decides where it lives: memory, disk or GCS).
        //    The payload carries a fresh ID so the execution step can refuse
        //    to push the same payload twice.
        let payload_id = uuid::Uuid::new_v4().to_string();
        let parsed = json!({
            "status": "success",
            PAYLOAD_ID_FIELD: payload_id,
            PAYLOAD_SOURCE_CSV_FIELD: source_csv,
            "profile_id": meta.profile_id,
            "advertiser_id": meta.advertiser_id,
            "campaign_id": meta.campaign_id,
            "campaign_name": meta.campaign_name,
            "operations": operations,
        });
        let artifacts = ctx
            .artifacts()
            .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
        let version = artifacts
            .save(
                PAYLOAD_ARTIFACT_NAME,
                &Part::Text {
                    text: serde_json::to_string_pretty(&parsed)?,
                },
            )
            .await?;
        let mut actions = ctx.actions();
        actions
            .artifact_delta
            .insert(PAYLOAD_ARTIFACT_NAME.to_string(), version);
        actions.state_delta.insert(
            PARSED_PAYLOAD_ARTIFACT_KEY.to_string(),
            Value::String(PAYLOAD_ARTIFACT_NAME.to_string()),
        );
        ctx.set_actions(actions);
        tracing::info!(
            "📦 [parse_sheet_tool] - Staged payload as artifact '{PAYLOAD_ARTIFACT_NAME}' (version {version})."
        );

        Ok(pretty(&json!({
            "status": "SUCCESS",
            "message": "The trafficking sheet has been successfully parsed.",
            "parsed_payload_artifact": PAYLOAD_ARTIFACT_NAME,
            PAYLOAD_ID_FIELD: payload_id,
            PAYLOAD_SOURCE_CSV_FIELD: source_csv,
            "profile_id": meta.profile_id,
            "advertiser_id": meta.advertiser_id,
            "campaign_id": meta.campaign_id,
            "campaign_name": meta.campaign_name,
            "sheet_entities": {
                "placements": placements,
                "ads": ads,
                "creatives": creatives,
                "event_tags": event_tags,
            },
            "operations": operations,
        })))
    }
}

/// Pushes the staged operations to CM360.
pub struct TrafficCampaignsTool {
    services: Cm360Services,
    /// IDs of the payloads this process has already started pushing. The
    /// session-state guards ([`PARSED_PAYLOAD_ARTIFACT_KEY`] /
    /// [`EXECUTED_PAYLOAD_ID_KEY`]) only take effect once a turn's state
    /// delta is committed; this set also stops a second call within the same
    /// turn (model retry, parallel function calls).
    executed_payloads: Mutex<HashSet<String>>,
}

impl TrafficCampaignsTool {
    /// Creates the tool.
    pub fn new(services: Cm360Services) -> Self {
        Self {
            services,
            executed_payloads: Mutex::new(HashSet::new()),
        }
    }

    /// Claims `payload_id` for execution; false when it was already claimed.
    fn claim_payload(&self, payload_id: &str) -> bool {
        let mut executed = self
            .executed_payloads
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if executed.len() >= MAX_REMEMBERED_PAYLOADS {
            executed.clear();
        }
        executed.insert(payload_id.to_string())
    }
}

/// Upper bound of the in-process executed-payload set (IDs are UUIDs, so this
/// is only hygiene for very long-lived processes; the session-state guards
/// remain authoritative).
const MAX_REMEMBERED_PAYLOADS: usize = 4096;

#[async_trait]
impl Tool for TrafficCampaignsTool {
    fn name(&self) -> &str {
        "traffic_campaigns_in_cm360_tool"
    }

    fn description(&self) -> &str {
        "Pushes the campaign trafficking operations staged by parse_sheet_tool to Campaign Manager 360: \
         executes placement/creative patches, event tag inserts/updates and ad inserts/updates \
         sequentially, marks rows whose entities were all trafficked as 'Trafficked' in the session \
         CSV artifact, and returns an execution summary. Only call this after the user has explicitly \
         approved the parsed operations. Each staged payload can be pushed only once: after this call \
         (successful or not) parse_sheet_tool must be run again before trafficking again. Returns \
         status AUTH_REQUIRED with an authorization URL when the user must first grant CM360 access."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
        tracing::info!("🚀 Starting traffic_campaigns_in_cm360_tool...");
        if let Err(required) = self.services.ensure_authorized(&ctx).await {
            return Ok(required.to_tool_response());
        }
        match self.run(&ctx).await {
            Ok(value) => Ok(value),
            Err(e) => {
                if let Some(required) = auth_required(&e) {
                    return Ok(required.to_tool_response());
                }
                tracing::error!(
                    "❌ [traffic_campaigns_in_cm360_tool] - Error executing CM360 trafficking push: {e:#}"
                );
                Ok(error_json(
                    "traffic_campaigns_in_cm360_tool",
                    format!("Error executing CM360 trafficking push: {e:#}"),
                ))
            }
        }
    }
}

/// Outcome of the post-execution trafficking sheet update, reported to the
/// model as the `trafficking_sheet` field of the execution summary.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SheetUpdate {
    /// `updated_rows` rows of `artifact` were marked `Trafficked`.
    Updated {
        artifact: String,
        updated_rows: usize,
    },
    /// Nothing to mark (no successful ad / placement operation) or the sheet
    /// lacks the status columns.
    Skipped { reason: String },
    /// The artifact could not be read or written back.
    Error { message: String },
}

impl SheetUpdate {
    fn to_json(&self) -> Value {
        match self {
            Self::Updated {
                artifact,
                updated_rows,
            } => json!({
                "status": "UPDATED",
                "artifact": artifact,
                "updated_rows": updated_rows,
                "message": format!(
                    "{updated_rows} row(s) marked 'Trafficked' in artifact '{artifact}'."
                ),
            }),
            Self::Skipped { reason } => json!({"status": "SKIPPED", "message": reason}),
            Self::Error { message } => json!({
                "status": "ERROR",
                "message": format!(
                    "The trafficking sheet artifact could not be updated; its rows still show \
                     their previous status even where CM360 was changed: {message}"
                ),
            }),
        }
    }
}

impl TrafficCampaignsTool {
    /// Loads the staged payload named by [`PARSED_PAYLOAD_ARTIFACT_KEY`].
    ///
    /// Fails closed when the key is absent / cleared (nothing is staged, or
    /// the staged payload was already executed) and when the payload's
    /// [`PAYLOAD_ID_FIELD`] matches [`EXECUTED_PAYLOAD_ID_KEY`], so a model
    /// retry can never re-send the same inserts.
    async fn load_payload(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<Value> {
        let Some(name) = state_get_str(ctx, PARSED_PAYLOAD_ARTIFACT_KEY) else {
            anyhow::bail!(
                "No staged CM360 payload for this session (either parse_sheet_tool has not run yet \
                 or the staged payload was already pushed). Run parse_sheet_tool, present the \
                 summary and obtain approval again before trafficking."
            );
        };
        let artifacts = ctx
            .artifacts()
            .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
        tracing::info!(
            "📥 [traffic_campaigns_in_cm360_tool] - Loading staged payload from artifact '{name}'..."
        );
        let part = artifacts.load(&name).await.map_err(|_| {
            anyhow::anyhow!("CM360 payloads not found (artifact '{name}'): run parse_sheet_tool first, then try again.")
        })?;
        let text = match part {
            Part::Text { text } => text,
            Part::InlineData { data, .. } => String::from_utf8_lossy(&data).to_string(),
            _ => anyhow::bail!(
                "Staged payload artifact '{name}' has an unexpected format. Please parse the sheet again."
            ),
        };
        let payload: Value = serde_json::from_str(&text)?;
        let payload_id = payload.get(PAYLOAD_ID_FIELD).and_then(Value::as_str);
        if let Some(payload_id) = payload_id.filter(|id| !id.is_empty())
            && state_get_str(ctx, EXECUTED_PAYLOAD_ID_KEY).as_deref() == Some(payload_id)
        {
            anyhow::bail!(
                "The staged CM360 payload ({payload_id}) was already pushed to CM360. Run \
                 parse_sheet_tool again to stage the remaining changes before trafficking again."
            );
        }
        Ok(payload)
    }

    async fn run(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<Value> {
        if ctx.session_id().is_empty() {
            anyhow::bail!("Session ID is required and must be present in the tool context.");
        }
        let payload = self.load_payload(ctx).await?;
        let get = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string);
        let payload_id = get(PAYLOAD_ID_FIELD);
        if let Some(payload_id) = &payload_id
            && !self.claim_payload(payload_id)
        {
            anyhow::bail!(
                "The staged CM360 payload ({payload_id}) was already pushed to CM360 in this \
                 session. Run parse_sheet_tool again to stage the remaining changes before \
                 trafficking again."
            );
        }
        let source_csv = get(PAYLOAD_SOURCE_CSV_FIELD);
        let profile_id = get("profile_id").unwrap_or_default();
        let advertiser_id = get("advertiser_id");
        let campaign_id = get("campaign_id");
        let campaign_name = get("campaign_name");
        let operations: Vec<super::diff::Operation> = serde_json::from_value(
            payload
                .get("operations")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )?;
        tracing::info!(
            "🚀 [traffic_campaigns_in_cm360_tool] - Pushing {} operations to CM360 for profile ID: {profile_id} (Advertiser ID: {advertiser_id:?}, Campaign ID: {campaign_id:?}, Campaign Name: {campaign_name:?})",
            operations.len()
        );

        let api = self.services.api_for(ctx);
        let partitioned = partition_operations(&operations);
        let placement_results =
            process_placement_operations(api.as_ref(), &profile_id, &partitioned.placement_ops)
                .await?;
        let creative_results =
            process_creative_operations(api.as_ref(), &profile_id, &partitioned.creative_ops)
                .await?;
        let (tag_results, event_tag_mappings, failed_event_tags) =
            process_event_tag_operations(api.as_ref(), &profile_id, &partitioned.event_tag_ops)
                .await;
        let ad_results = process_ad_operations(
            api.as_ref(),
            &profile_id,
            &partitioned.ad_ops,
            &event_tag_mappings,
            &failed_event_tags,
        )
        .await;

        let mut all_results = placement_results;
        all_results.extend(creative_results);
        all_results.extend(tag_results);
        all_results.extend(ad_results);

        // The payload has been pushed (however it went): consume it so a
        // retry cannot re-send the same inserts, and remember its ID.
        let mut actions = ctx.actions();
        actions
            .state_delta
            .insert(PARSED_PAYLOAD_ARTIFACT_KEY.to_string(), Value::Null);
        if let Some(payload_id) = &payload_id {
            actions.state_delta.insert(
                EXECUTED_PAYLOAD_ID_KEY.to_string(),
                Value::String(payload_id.clone()),
            );
        }
        ctx.set_actions(actions);

        // Update the trafficking sheet artifact status to 'Trafficked'.
        let sheet_update = match self
            .update_sheet_status(ctx, source_csv.as_deref(), &all_results)
            .await
        {
            Ok(update) => update,
            Err(e) => {
                tracing::warn!(
                    "⚠️ [traffic_campaigns_in_cm360_tool] - Could not update the trafficking sheet artifact: {e:#}"
                );
                SheetUpdate::Error {
                    message: format!("{e:#}"),
                }
            }
        };

        let summary = build_trafficking_summary_response(
            &all_results,
            operations.len(),
            Some(&profile_id),
            advertiser_id.as_deref(),
            campaign_id.as_deref(),
            campaign_name.as_deref(),
        );
        let mut response: Value = serde_json::from_str(&summary).unwrap_or(Value::String(summary));
        if let Some(obj) = response.as_object_mut() {
            obj.insert("trafficking_sheet".to_string(), sheet_update.to_json());
        }
        Ok(response)
    }

    /// Marks the fully trafficked rows of the source CSV artifact (the one
    /// recorded at parse time; the latest CSV of the session when a legacy
    /// payload carries no [`PAYLOAD_SOURCE_CSV_FIELD`]).
    async fn update_sheet_status(
        &self,
        ctx: &Arc<dyn ToolContext>,
        source_csv: Option<&str>,
        results: &[super::execute::OperationResult],
    ) -> anyhow::Result<SheetUpdate> {
        let outcomes = entity_outcomes(results);
        if !outcomes.has_successes() {
            tracing::info!(
                "ℹ️ [traffic_campaigns_in_cm360_tool] - No successful ad or placement operations to update in trafficking sheet."
            );
            return Ok(SheetUpdate::Skipped {
                reason: "No successful ad or placement operations to mark.".to_string(),
            });
        }
        let filename = match source_csv {
            Some(name) => name.to_string(),
            None => {
                tracing::warn!(
                    "⚠️ [traffic_campaigns_in_cm360_tool] - Staged payload does not name its source CSV; falling back to the latest CSV artifact."
                );
                latest_csv_name(ctx).await?
            }
        };
        let raw = load_raw_sheet(ctx, &filename).await?;
        let Some((updated, count)) = apply_trafficked_status(&raw, &outcomes) else {
            return Ok(SheetUpdate::Skipped {
                reason: format!(
                    "Artifact '{filename}' has no 'Trafficking Status' / 'Placement Name' / \
                     'Ad Name' columns to update."
                ),
            });
        };
        let csv = to_csv(&updated)?;
        let artifacts = ctx
            .artifacts()
            .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
        let version = artifacts.save(&filename, &Part::Text { text: csv }).await?;
        let mut actions = ctx.actions();
        actions.artifact_delta.insert(filename.clone(), version);
        ctx.set_actions(actions);
        tracing::info!(
            "📄 [traffic_campaigns_in_cm360_tool] - Successfully updated {count} row(s) to 'Trafficked' in artifact '{filename}'."
        );
        Ok(SheetUpdate::Updated {
            artifact: filename,
            updated_rows: count,
        })
    }
}

/// Toolset for parsing trafficking sheets and executing trafficking.
pub struct Cm360TraffickingToolset {
    tools: Vec<Arc<dyn Tool>>,
}

impl Cm360TraffickingToolset {
    /// Creates the toolset.
    pub fn new(services: Cm360Services) -> Self {
        Self {
            tools: vec![
                Arc::new(ParseSheetTool::new(services.clone())),
                Arc::new(TrafficCampaignsTool::new(services)),
            ],
        }
    }
}

#[async_trait]
impl Toolset for Cm360TraffickingToolset {
    fn name(&self) -> &str {
        "cm360_trafficking"
    }

    async fn tools(&self, _ctx: Arc<dyn ReadonlyContext>) -> Result<Vec<Arc<dyn Tool>>> {
        Ok(self.tools.clone())
    }

    async fn process_llm_request(
        &self,
        _ctx: Arc<dyn ReadonlyContext>,
        request: &mut LlmRequest,
    ) -> Result<()> {
        request.contents.insert(
            0,
            Content::new("user").with_text(CM360_TRAFFICKING_INSTRUCTIONS),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use adk_rust::{Artifacts, CallbackContext, EventActions, MemoryEntry, Session, State};
    use anyhow::anyhow;
    use indexmap::IndexMap;

    use super::super::client::test_support::MockApi;
    use super::super::sheet::test_support::make_test_csv;
    use super::*;
    use crate::auth::{GoogleOAuthFlow, OAuthClientConfig};

    /// Versioned in-memory artifact store shaped like the runner's.
    #[derive(Default)]
    struct MemoryArtifacts {
        files: Mutex<IndexMap<String, Vec<Part>>>,
    }

    impl MemoryArtifacts {
        fn text(&self, name: &str) -> Option<String> {
            match self.files.lock().unwrap().get(name)?.last()? {
                Part::Text { text } => Some(text.clone()),
                _ => None,
            }
        }

        fn versions(&self, name: &str) -> usize {
            self.files.lock().unwrap().get(name).map_or(0, Vec::len)
        }
    }

    #[async_trait]
    impl Artifacts for MemoryArtifacts {
        async fn save(&self, name: &str, data: &Part) -> Result<i64> {
            let mut files = self.files.lock().unwrap();
            let versions = files.entry(name.to_string()).or_default();
            versions.push(data.clone());
            Ok((versions.len() - 1) as i64)
        }
        async fn load(&self, name: &str) -> Result<Part> {
            self.files
                .lock()
                .unwrap()
                .get(name)
                .and_then(|v| v.last().cloned())
                .ok_or_else(|| adk_rust::AdkError::tool(format!("artifact {name} not found")))
        }
        async fn list(&self) -> Result<Vec<String>> {
            Ok(self.files.lock().unwrap().keys().cloned().collect())
        }
    }

    /// Tool context with committed session state, pending `EventActions`
    /// and an optional artifact store.
    struct FakeToolContext {
        state: Mutex<HashMap<String, Value>>,
        actions: Mutex<EventActions>,
        artifacts: Option<Arc<MemoryArtifacts>>,
        content: Content,
        session_id: String,
    }

    impl FakeToolContext {
        fn new(artifacts: Option<Arc<MemoryArtifacts>>) -> Arc<Self> {
            Arc::new(Self {
                state: Mutex::new(HashMap::new()),
                actions: Mutex::new(EventActions::default()),
                artifacts,
                content: Content::new("user"),
                session_id: "s-1".to_string(),
            })
        }

        /// Applies the pending `state_delta` to the committed state (what the
        /// runner does between turns) and clears the pending actions.
        fn commit(&self) {
            let mut actions = self.actions.lock().unwrap();
            let mut state = self.state.lock().unwrap();
            for (key, value) in actions.state_delta.drain() {
                state.insert(key, value);
            }
            *actions = EventActions::default();
        }

        fn state_value(&self, key: &str) -> Option<Value> {
            self.state.lock().unwrap().get(key).cloned()
        }

        fn pending_state(&self, key: &str) -> Option<Value> {
            self.actions.lock().unwrap().state_delta.get(key).cloned()
        }
    }

    impl State for FakeToolContext {
        fn get(&self, key: &str) -> Option<Value> {
            self.state.lock().unwrap().get(key).cloned()
        }
        fn set(&mut self, key: String, value: Value) {
            self.state.lock().unwrap().insert(key, value);
        }
        fn all(&self) -> HashMap<String, Value> {
            self.state.lock().unwrap().clone()
        }
    }

    impl ReadonlyContext for FakeToolContext {
        fn invocation_id(&self) -> &str {
            "inv"
        }
        fn agent_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "local-user"
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn session_id(&self) -> &str {
            &self.session_id
        }
        fn branch(&self) -> &str {
            ""
        }
        fn user_content(&self) -> &Content {
            &self.content
        }
    }

    impl Session for FakeToolContext {
        fn id(&self) -> &str {
            &self.session_id
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "local-user"
        }
        fn state(&self) -> &dyn State {
            self
        }
        fn conversation_history(&self) -> Vec<Content> {
            Vec::new()
        }
    }

    #[async_trait]
    impl CallbackContext for FakeToolContext {
        fn artifacts(&self) -> Option<Arc<dyn Artifacts>> {
            self.artifacts.clone().map(|a| a as Arc<dyn Artifacts>)
        }
    }

    #[async_trait]
    impl ToolContext for FakeToolContext {
        fn function_call_id(&self) -> &str {
            "call"
        }
        fn actions(&self) -> EventActions {
            self.actions.lock().unwrap().clone()
        }
        fn set_actions(&self, actions: EventActions) {
            *self.actions.lock().unwrap() = actions;
        }
        async fn search_memory(&self, _query: &str) -> Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        fn session(&self) -> Option<&dyn Session> {
            Some(self)
        }
    }

    fn credentials() -> Arc<SessionCredentialManager> {
        Arc::new(SessionCredentialManager::new(GoogleOAuthFlow::new(
            OAuthClientConfig {
                client_id: "cid".into(),
                client_secret: "secret".into(),
                redirect_uri: "http://localhost:8000/oauth2/callback".into(),
            },
        )))
    }

    fn services(api: Option<Arc<dyn Cm360Api>>) -> Cm360Services {
        Cm360Services {
            credentials: credentials(),
            api_override: api,
        }
    }

    fn tools(api: Arc<MockApi>) -> (ParseSheetTool, TrafficCampaignsTool) {
        let services = services(Some(api as Arc<dyn Cm360Api>));
        (
            ParseSheetTool::new(services.clone()),
            TrafficCampaignsTool::new(services),
        )
    }

    /// A mock whose CM360 state matches the test CSV except that the ad does
    /// not exist yet (so parsing yields exactly one `ads.insert`).
    fn mock_with_matching_placement_and_creative() -> MockApi {
        MockApi::new()
            .with_placements(vec![json!({
                "name": "Test~Placement~1 Test",
                "id": "p1",
                "activeStatus": "PLACEMENT_STATUS_ACTIVE",
                "size": {"width": 300, "height": 250},
                "pricingSchedule": {
                    "startDate": "3000-08-29",
                    "endDate": "3000-09-25",
                    "pricingPeriods": [{"startDate": "3000-08-29", "endDate": "3000-09-25"}],
                },
            })])
            .with_creatives(vec![json!({
                "name": "TEST_ACG~300x250",
                "id": "999123",
                "size": {"width": 300, "height": 250},
            })])
    }

    async fn upload_csv(artifacts: &MemoryArtifacts, name: &str, csv: &str) {
        artifacts
            .save(
                name,
                &Part::Text {
                    text: csv.to_string(),
                },
            )
            .await
            .unwrap();
    }

    fn status_cell(csv: &str) -> String {
        let raw = parse_csv(csv).unwrap();
        raw[5][2].clone()
    }

    #[tokio::test]
    async fn parse_then_traffic_flow_marks_sheet_and_consumes_payload() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts.clone()));
        let api = Arc::new(mock_with_matching_placement_and_creative());
        let (parse, traffic) = tools(api.clone());

        // Parse: stages the payload and records it in the state delta.
        let parsed = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(parsed["status"], "SUCCESS", "{parsed}");
        assert_eq!(parsed["parsed_payload_artifact"], PAYLOAD_ARTIFACT_NAME);
        assert_eq!(parsed[PAYLOAD_SOURCE_CSV_FIELD], "campaign.csv");
        assert_eq!(parsed["profile_id"], "7023449");
        let ops = parsed["operations"].as_array().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["operation"], "dfareporting.ads.insert");
        assert_eq!(ops[0]["name"], "Test Ad C");
        assert_eq!(
            parsed["sheet_entities"]["placements"]["Test~Placement~1 Test"]["pricingSchedule"]["startDate"],
            "3000-08-29"
        );
        let payload_id = parsed[PAYLOAD_ID_FIELD].as_str().unwrap().to_string();
        assert!(!payload_id.is_empty());
        assert_eq!(
            ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY),
            Some(json!(PAYLOAD_ARTIFACT_NAME))
        );
        let staged: Value =
            serde_json::from_str(&artifacts.text(PAYLOAD_ARTIFACT_NAME).unwrap()).unwrap();
        assert_eq!(staged[PAYLOAD_ID_FIELD], payload_id);
        assert_eq!(staged[PAYLOAD_SOURCE_CSV_FIELD], "campaign.csv");
        assert!(api.mutation_calls().is_empty());
        ctx.commit();

        // Traffic: pushes the ad, marks the row, consumes the payload.
        let summary = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(summary["status"], "SUCCESS", "{summary}");
        assert_eq!(summary["profile_id"], "7023449");
        assert_eq!(summary["results"][0]["name"], "Test Ad C");
        assert_eq!(summary["results"][0]["status"], "SUCCESS");
        assert_eq!(summary["trafficking_sheet"]["status"], "UPDATED");
        assert_eq!(summary["trafficking_sheet"]["artifact"], "campaign.csv");
        assert_eq!(summary["trafficking_sheet"]["updated_rows"], 1);
        let calls = api.mutation_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "insert_ad");
        assert_eq!(calls[0].profile_id, "7023449");
        assert_eq!(calls[0].body["name"], "Test Ad C");
        assert_eq!(
            calls[0].body["placementAssignments"][0]["placementId"],
            "p1"
        );
        assert_eq!(
            status_cell(&artifacts.text("campaign.csv").unwrap()),
            "Trafficked"
        );
        assert_eq!(artifacts.versions("campaign.csv"), 2);
        assert_eq!(
            ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY),
            Some(Value::Null)
        );
        assert_eq!(
            ctx.pending_state(EXECUTED_PAYLOAD_ID_KEY),
            Some(json!(payload_id))
        );
        assert_eq!(ctx.actions().artifact_delta.get("campaign.csv"), Some(&1));
    }

    #[tokio::test]
    async fn traffic_refuses_to_run_twice_on_the_same_payload() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts.clone()));
        let api = Arc::new(mock_with_matching_placement_and_creative());
        let (parse, traffic) = tools(api.clone());

        parse.execute(ctx.clone(), json!({})).await.unwrap();
        ctx.commit();
        let first = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(first["status"], "SUCCESS");
        assert_eq!(api.mutation_calls().len(), 1);

        // Same turn (state deltas not committed yet, so the committed state
        // still names the payload): the in-process guard trips.
        assert_eq!(
            ctx.state_value(PARSED_PAYLOAD_ARTIFACT_KEY),
            Some(json!(PAYLOAD_ARTIFACT_NAME))
        );
        let again = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(again["status"], "ERROR", "{again}");
        assert!(
            again["message"]
                .as_str()
                .unwrap()
                .contains("was already pushed"),
            "{again}"
        );
        assert_eq!(api.mutation_calls().len(), 1);

        // Another process / instance (fresh tool, no in-process memory) with
        // the committed deltas: the executed-ID guard trips even when the
        // staged-payload key somehow still names the artifact.
        let other_instance = TrafficCampaignsTool::new(services(Some(api.clone())));
        ctx.commit();
        assert_eq!(
            ctx.state_value(PARSED_PAYLOAD_ARTIFACT_KEY),
            Some(Value::Null)
        );
        assert!(ctx.state_value(EXECUTED_PAYLOAD_ID_KEY).is_some());
        {
            let mut state = ctx.state.lock().unwrap();
            state.insert(
                PARSED_PAYLOAD_ARTIFACT_KEY.to_string(),
                json!(PAYLOAD_ARTIFACT_NAME),
            );
        }
        let stale = other_instance
            .execute(ctx.clone(), json!({}))
            .await
            .unwrap();
        assert_eq!(stale["status"], "ERROR", "{stale}");
        assert!(
            stale["message"]
                .as_str()
                .unwrap()
                .contains("was already pushed"),
            "{stale}"
        );
        assert_eq!(api.mutation_calls().len(), 1);

        // With the key cleared (the normal committed state) nothing is staged.
        {
            let mut state = ctx.state.lock().unwrap();
            state.insert(PARSED_PAYLOAD_ARTIFACT_KEY.to_string(), Value::Null);
        }
        let third = other_instance
            .execute(ctx.clone(), json!({}))
            .await
            .unwrap();
        assert_eq!(third["status"], "ERROR");
        assert!(
            third["message"]
                .as_str()
                .unwrap()
                .contains("No staged CM360 payload"),
            "{third}"
        );
        assert_eq!(api.mutation_calls().len(), 1);

        // A fresh parse re-arms the tool (the row is Trafficked now, so the
        // sheet has nothing left to push).
        let reparsed = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(reparsed["status"], "SUCCESS");
        assert!(reparsed["operations"].as_array().unwrap().is_empty());
        ctx.commit();
        let fourth = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(fourth["status"], "SUCCESS", "{fourth}");
        assert_eq!(fourth["trafficking_sheet"]["status"], "SKIPPED");
        assert_eq!(api.mutation_calls().len(), 1);
    }

    #[tokio::test]
    async fn traffic_without_staged_payload_is_an_error_without_api_calls() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        // A stale payload artifact exists but nothing is staged in state.
        upload_csv(&artifacts, PAYLOAD_ARTIFACT_NAME, r#"{"operations": []}"#).await;
        let ctx = FakeToolContext::new(Some(artifacts));
        let api = Arc::new(MockApi::new());
        let (_, traffic) = tools(api.clone());
        let result = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "ERROR");
        assert!(
            result["message"]
                .as_str()
                .unwrap()
                .contains("No staged CM360 payload"),
            "{result}"
        );
        assert!(api.mutation_calls().is_empty());
        assert!(ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY).is_none());
    }

    #[tokio::test]
    async fn failed_ad_keeps_row_retryable_until_a_later_run_succeeds() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts.clone()));

        // Run 1: the placement needs a patch (status differs) which succeeds,
        // the ad insert fails.
        let mut mock = mock_with_matching_placement_and_creative();
        mock.placements[0]["activeStatus"] = json!("PLACEMENT_STATUS_INACTIVE");
        let api = Arc::new(mock.with_responder(|call| match call.method {
            "insert_ad" => Err(anyhow!("backend exploded")),
            _ => Ok(json!({"id": call.id.clone().unwrap_or_default()})),
        }));
        let (parse, traffic) = tools(api.clone());
        let parsed = parse.execute(ctx.clone(), json!({})).await.unwrap();
        let op_names: Vec<&str> = parsed["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| op["operation"].as_str().unwrap())
            .collect();
        assert_eq!(
            op_names,
            vec!["dfareporting.placements.patch", "dfareporting.ads.insert"]
        );
        ctx.commit();
        let summary = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(summary["status"], "PARTIAL_SUCCESS", "{summary}");
        assert!(
            summary["message"]
                .as_str()
                .unwrap()
                .contains("run parse_sheet_tool again")
        );
        assert_eq!(summary["results"][1]["status"], "ERROR");
        assert_eq!(
            summary["results"][1]["message"],
            "Execution failed: backend exploded"
        );
        // The placement succeeded but the row's ad failed → row stays New.
        assert_eq!(summary["trafficking_sheet"]["status"], "UPDATED");
        assert_eq!(summary["trafficking_sheet"]["updated_rows"], 0);
        assert_eq!(status_cell(&artifacts.text("campaign.csv").unwrap()), "New");
        ctx.commit();

        // Run 2: CM360 now matches on the placement; a fresh parse stages
        // only the ad insert, which succeeds this time.
        let api = Arc::new(mock_with_matching_placement_and_creative());
        let (parse, traffic) = tools(api.clone());
        let parsed = parse.execute(ctx.clone(), json!({})).await.unwrap();
        let ops = parsed["operations"].as_array().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["operation"], "dfareporting.ads.insert");
        ctx.commit();
        let summary = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(summary["status"], "SUCCESS", "{summary}");
        assert_eq!(summary["trafficking_sheet"]["updated_rows"], 1);
        assert_eq!(
            status_cell(&artifacts.text("campaign.csv").unwrap()),
            "Trafficked"
        );
        assert_eq!(api.mutation_calls().len(), 1);
        assert_eq!(api.mutation_calls()[0].method, "insert_ad");
    }

    #[tokio::test]
    async fn sheet_update_failure_is_surfaced_in_the_summary() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts.clone()));
        let api = Arc::new(mock_with_matching_placement_and_creative());
        let (parse, traffic) = tools(api.clone());
        parse.execute(ctx.clone(), json!({})).await.unwrap();
        ctx.commit();
        // The source CSV disappears between parse and traffic.
        artifacts.files.lock().unwrap().shift_remove("campaign.csv");

        let summary = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        // CM360 was updated...
        assert_eq!(summary["status"], "SUCCESS", "{summary}");
        assert_eq!(api.mutation_calls().len(), 1);
        // ...but the model is told the sheet could not be.
        assert_eq!(summary["trafficking_sheet"]["status"], "ERROR");
        assert!(
            summary["trafficking_sheet"]["message"]
                .as_str()
                .unwrap()
                .contains("campaign.csv"),
            "{summary}"
        );
        // The payload is consumed regardless.
        assert_eq!(
            ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY),
            Some(Value::Null)
        );
    }

    #[tokio::test]
    async fn parse_reports_validation_errors_and_missing_csv() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        let ctx = FakeToolContext::new(Some(artifacts.clone()));
        let api = Arc::new(MockApi::new());
        let (parse, _) = tools(api.clone());

        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "ERROR");
        assert!(
            result["message"]
                .as_str()
                .unwrap()
                .contains("No CSV trafficking files"),
            "{result}"
        );

        upload_csv(
            &artifacts,
            "campaign.csv",
            &make_test_csv(&[("ad_name", "")]),
        )
        .await;
        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "error");
        assert_eq!(result["validation_errors"][0]["field"], "Ad Name");
        assert!(api.list_calls().is_empty());
        assert!(ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY).is_none());

        // The latest CSV is the one that gets parsed.
        upload_csv(&artifacts, "newer.csv", &make_test_csv(&[])).await;
        let api = Arc::new(mock_with_matching_placement_and_creative());
        let (parse, _) = tools(api);
        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "SUCCESS", "{result}");
        assert_eq!(result[PAYLOAD_SOURCE_CSV_FIELD], "newer.csv");
    }

    #[tokio::test]
    async fn parse_without_artifact_service_is_an_error() {
        let ctx = FakeToolContext::new(None);
        let (parse, traffic) = tools(Arc::new(MockApi::new()));
        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "ERROR");
        assert!(
            result["message"]
                .as_str()
                .unwrap()
                .contains("Artifact service is not configured")
        );
        let result = traffic.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "ERROR");
    }

    #[tokio::test]
    async fn auth_required_is_surfaced_before_any_api_call() {
        // Without an API override the tools ask the credential manager, which
        // has no token for the session and (with no interactive wait) returns
        // AuthRequired straight away.
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts));
        let services = services(None);
        let parse = ParseSheetTool::new(services.clone());
        let traffic = TrafficCampaignsTool::new(services);

        for tool in [&parse as &dyn Tool, &traffic as &dyn Tool] {
            let result = tool.execute(ctx.clone(), json!({})).await.unwrap();
            assert_eq!(result["status"], "AUTH_REQUIRED", "{result}");
            assert_eq!(result["pending"], true);
            assert!(
                result["auth_url"]
                    .as_str()
                    .unwrap()
                    .contains("dfatrafficking"),
                "{result}"
            );
            assert_eq!(result["scopes"], json!(Cm360Services::scopes()));
        }
        assert!(ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY).is_none());
    }

    #[tokio::test]
    async fn auth_required_raised_mid_run_is_surfaced() {
        let artifacts = Arc::new(MemoryArtifacts::default());
        upload_csv(&artifacts, "campaign.csv", &make_test_csv(&[])).await;
        let ctx = FakeToolContext::new(Some(artifacts.clone()));
        let required = AuthRequired {
            auth_url: "https://accounts.example/auth".into(),
            scopes: Cm360Services::scopes(),
            message: "Please authorize CM360 access.".into(),
        };
        let api = Arc::new(
            MockApi::new().with_list_failure(move || anyhow::Error::new(required.clone())),
        );
        let (parse, _) = tools(api.clone());

        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "AUTH_REQUIRED", "{result}");
        assert_eq!(result["auth_url"], "https://accounts.example/auth");
        assert_eq!(result["message"], "Please authorize CM360 access.");
        // The failure happened in the first list call; nothing was staged.
        assert_eq!(api.list_calls().len(), 1);
        assert!(artifacts.text(PAYLOAD_ARTIFACT_NAME).is_none());
        assert!(ctx.pending_state(PARSED_PAYLOAD_ARTIFACT_KEY).is_none());

        // A plain API failure is reported as a tool error with the chain.
        let api = Arc::new(MockApi::new().with_list_error("quota exceeded"));
        let (parse, _) = tools(api);
        let result = parse.execute(ctx.clone(), json!({})).await.unwrap();
        assert_eq!(result["status"], "ERROR");
        let message = result["message"].as_str().unwrap();
        assert!(message.contains("Failed to fetch placements"), "{message}");
        assert!(message.contains("quota exceeded"), "{message}");
    }

    #[tokio::test]
    async fn toolset_exposes_both_tools_and_injects_instructions() {
        let toolset = Cm360TraffickingToolset::new(services(Some(Arc::new(MockApi::new()))));
        assert_eq!(toolset.name(), "cm360_trafficking");
        let ctx = FakeToolContext::new(None);
        let tools = toolset
            .tools(ctx.clone() as Arc<dyn ReadonlyContext>)
            .await
            .unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            vec!["parse_sheet_tool", "traffic_campaigns_in_cm360_tool"]
        );
        for tool in &tools {
            assert!(tool.description().contains("AUTH_REQUIRED"));
            assert_eq!(
                tool.parameters_schema(),
                Some(json!({"type": "object", "properties": {}}))
            );
        }
        let mut request = LlmRequest::new("model", Vec::new());
        toolset
            .process_llm_request(ctx as Arc<dyn ReadonlyContext>, &mut request)
            .await
            .unwrap();
        assert_eq!(request.contents.len(), 1);
        assert!(format!("{:?}", services(None)).contains("Cm360Services"));
    }
}
