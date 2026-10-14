//! ADK tools for parsing CM360 trafficking sheets and executing trafficking.
//!
//! The workflow guidance that used to live in a separate `cm360-trafficking`
//! skill is embedded here: the toolset injects it into every model request
//! through [`Toolset::process_llm_request`], and the hard rules are repeated
//! in the tool descriptions, so there is a single source of truth for the
//! CM360 trafficking behaviour.

use std::collections::HashSet;
use std::sync::Arc;

use adk_rust::{Content, LlmRequest, Part, ReadonlyContext, Result, Tool, ToolContext, Toolset};
use async_trait::async_trait;
use serde_json::{Value, json};

use super::client::{Cm360Api, Cm360Client, SCOPES};
use super::diff::{resolve_and_build_operations, resolve_creative_ids, resolve_placement_ids};
use super::execute::{
    build_trafficking_summary_response, partition_operations, process_ad_operations,
    process_creative_operations, process_event_tag_operations, process_placement_operations,
    successful_entity_names,
};
use super::grouping::{group_ads, group_creatives, group_event_tags, group_placements};
use super::sheet::{RawSheet, apply_trafficked_status, parse_csv, prepare_data, to_csv};
use super::validations::validate_sheet;
use crate::auth::{AuthRequired, UserCredentialManager, UserTokenProvider, auth_required};
use crate::gcs::{GcsClient, parse_gs_url};

/// Session-state key holding the location of the parsed payload.
pub const PARSED_PAYLOAD_URL_KEY: &str = "parsed_payload_gcs_url";
/// Session-state key holding the parsed payload itself when no GCS bucket is
/// configured.
pub const PARSED_PAYLOAD_INLINE_KEY: &str = "cm360_parsed_payload";
/// Marker URL used when the payload is stored inline in session state.
const INLINE_PAYLOAD_URL: &str = "session-state://cm360_parsed_payload";

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
- **Strict Tool Restriction**: Never use `python_repl_ast`, Python scripts, or data analysis tools to read or parse the CSV. Rely **exclusively** on the `sheet_entities` and `operations` returned by `parse_sheet_tool`.
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

- After the user gives explicit approval, call **`traffic_campaigns_in_cm360_tool`**.
- Present the execution results for **Placements**, **Ads**, **Creatives**, and **Event Tags** (indicate `None` if a section had no executed operations, and note when creative rotations were applied at the Ad level), followed by instructions to download the updated `Trafficked` CSV artifact:

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
    /// Per-user Google OAuth credentials.
    pub credentials: Arc<UserCredentialManager>,
    /// GCS client (ADC) and bucket for parsed payloads; `None` stores the
    /// payload in session state instead.
    pub payload_bucket: Option<(GcsClient, String)>,
    /// Optional override of the CM360 API (tests).
    pub api_override: Option<Arc<dyn Cm360Api>>,
}

impl std::fmt::Debug for Cm360Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cm360Services")
            .field(
                "payload_bucket",
                &self.payload_bucket.as_ref().map(|(_, b)| b),
            )
            .finish_non_exhaustive()
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
            UserTokenProvider::new(self.credentials.clone(), ctx.user_id(), Self::scopes());
        Arc::new(Cm360Client::new(Arc::new(provider)))
    }

    /// Ensures the user has valid CM360 credentials before any API call.
    async fn ensure_authorized(
        &self,
        ctx: &Arc<dyn ToolContext>,
    ) -> std::result::Result<(), AuthRequired> {
        if self.api_override.is_some() {
            return Ok(());
        }
        match self
            .credentials
            .access_token(ctx.user_id(), &Self::scopes())
            .await
        {
            Ok(_) => Ok(()),
            Err(e) => match auth_required(&e) {
                Some(required) => Err(required.clone()),
                None => Err(AuthRequired {
                    auth_url: String::new(),
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

/// Loads the latest CSV artifact of the session as `(raw_sheet, filename)`.
async fn load_raw_sheet(ctx: &Arc<dyn ToolContext>) -> anyhow::Result<(RawSheet, String)> {
    let artifacts = ctx
        .artifacts()
        .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
    let files = artifacts.list().await?;
    tracing::info!("📂 Discovered artifacts in session: {files:?}");
    let target = files
        .iter()
        .rfind(|f| f.to_lowercase().ends_with(".csv"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No CSV trafficking files (.csv) found in session artifacts. Please upload your trafficking CSV file."))?;
    tracing::info!("🎯 Selecting target campaign file: {target}");
    let part = artifacts
        .load(&target)
        .await
        .map_err(|e| anyhow::anyhow!("Artifact '{target}' could not be loaded: {e}"))?;
    let text = match part {
        Part::Text { text } => text,
        Part::InlineData { data, .. } => String::from_utf8_lossy(&data).to_string(),
        _ => anyhow::bail!("Failed to extract data from artifact '{target}'."),
    };
    Ok((parse_csv(&text)?, target))
}

fn state_get(ctx: &Arc<dyn ToolContext>, key: &str) -> Option<Value> {
    ctx.session().and_then(|s| s.state().get(key))
}

fn state_set(ctx: &Arc<dyn ToolContext>, key: &str, value: Value) {
    let mut actions = ctx.actions();
    actions.state_delta.insert(key.to_string(), value);
    ctx.set_actions(actions);
}

/// Parses a CM360 trafficking sheet and stages the resolved operations.
pub struct ParseSheetTool {
    services: Cm360Services,
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
         the CSV with python_repl_ast or other tools; rely exclusively on this tool's `sheet_entities` \
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
        let session_id = ctx.session_id().to_string();
        if session_id.is_empty() {
            return Ok(error_json(
                "parse_sheet_tool",
                "Session ID is required and must be present in the tool context.",
            ));
        }
        match self.run(&ctx, &session_id).await {
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
    async fn run(&self, ctx: &Arc<dyn ToolContext>, session_id: &str) -> anyhow::Result<Value> {
        // 1. Load the raw sheet.
        let (raw, _filename) = load_raw_sheet(ctx).await?;

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

        // 7. Store the parsed payload.
        let parsed = json!({
            "status": "success",
            "profile_id": meta.profile_id,
            "advertiser_id": meta.advertiser_id,
            "campaign_id": meta.campaign_id,
            "campaign_name": meta.campaign_name,
            "operations": operations,
        });
        let payload_url = match &self.services.payload_bucket {
            Some((gcs, bucket)) => {
                let object_path = format!("cm360_trafficking/{session_id}/payloads.json");
                let content = serde_json::to_string_pretty(&parsed)?;
                gcs.upload_string(bucket, &object_path, &content, "application/json")
                    .await?
            }
            None => {
                tracing::warn!(
                    "SKILLS_BUCKET_NAME is not set; storing the CM360 payload in session state instead of GCS."
                );
                state_set(ctx, PARSED_PAYLOAD_INLINE_KEY, parsed.clone());
                INLINE_PAYLOAD_URL.to_string()
            }
        };
        state_set(
            ctx,
            PARSED_PAYLOAD_URL_KEY,
            Value::String(payload_url.clone()),
        );

        Ok(pretty(&json!({
            "status": "SUCCESS",
            "message": "The trafficking sheet has been successfully parsed.",
            "parsed_payload_gcs_url": payload_url,
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
}

#[async_trait]
impl Tool for TrafficCampaignsTool {
    fn name(&self) -> &str {
        "traffic_campaigns_in_cm360_tool"
    }

    fn description(&self) -> &str {
        "Pushes the campaign trafficking operations staged by parse_sheet_tool to Campaign Manager 360: \
         executes placement/creative patches, event tag inserts/updates and ad inserts/updates \
         sequentially, marks successfully trafficked rows as 'Trafficked' in the session CSV artifact, \
         and returns an execution summary. Only call this after the user has explicitly approved the \
         parsed operations. Returns status AUTH_REQUIRED with an authorization URL when the user must \
         first grant CM360 access."
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

impl TrafficCampaignsTool {
    async fn load_payload(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<Value> {
        let url =
            state_get(ctx, PARSED_PAYLOAD_URL_KEY).and_then(|v| v.as_str().map(str::to_string));
        let Some(url) = url else {
            anyhow::bail!(
                "CM360 payloads not found: parse_sheet_tool has not been run in this session. Please parse the sheet first."
            );
        };
        if url == INLINE_PAYLOAD_URL {
            return state_get(ctx, PARSED_PAYLOAD_INLINE_KEY).ok_or_else(|| {
                anyhow::anyhow!(
                    "CM360 payloads not found in session state. Please try parsing the sheet again."
                )
            });
        }
        let Some((bucket, object_path)) = parse_gs_url(&url) else {
            anyhow::bail!(
                "CM360 payloads not found in GCS: {url}. Please try parsing the sheet again."
            );
        };
        let Some((gcs, _)) = &self.services.payload_bucket else {
            anyhow::bail!(
                "GCS bucket name could not be determined from environment or parsed_payload_gcs_url: {url}"
            );
        };
        tracing::info!(
            "📥 [traffic_campaigns_in_cm360_tool] - Downloading payloads from GCS: {url}..."
        );
        let content = gcs.download_string(bucket, object_path).await?;
        Ok(serde_json::from_str(&content)?)
    }

    async fn run(&self, ctx: &Arc<dyn ToolContext>) -> anyhow::Result<Value> {
        if ctx.session_id().is_empty() {
            anyhow::bail!("Session ID is required and must be present in the tool context.");
        }
        let payload = self.load_payload(ctx).await?;
        let get = |key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string);
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

        // Update the trafficking sheet artifact status to 'Trafficked'.
        if let Err(e) = self.update_sheet_status(ctx, &all_results).await {
            tracing::warn!(
                "⚠️ [traffic_campaigns_in_cm360_tool] - Could not update the trafficking sheet artifact: {e:#}"
            );
        }

        let summary = build_trafficking_summary_response(
            &all_results,
            operations.len(),
            Some(&profile_id),
            advertiser_id.as_deref(),
            campaign_id.as_deref(),
            campaign_name.as_deref(),
        );
        Ok(serde_json::from_str(&summary).unwrap_or(Value::String(summary)))
    }

    async fn update_sheet_status(
        &self,
        ctx: &Arc<dyn ToolContext>,
        results: &[super::execute::OperationResult],
    ) -> anyhow::Result<()> {
        let (successful_ads, successful_placements): (HashSet<String>, HashSet<String>) =
            successful_entity_names(results);
        if successful_ads.is_empty() && successful_placements.is_empty() {
            tracing::info!(
                "ℹ️ [traffic_campaigns_in_cm360_tool] - No successful ad or placement operations to update in trafficking sheet."
            );
            return Ok(());
        }
        let (raw, filename) = load_raw_sheet(ctx).await?;
        let Some((updated, count)) =
            apply_trafficked_status(&raw, &successful_ads, &successful_placements)
        else {
            return Ok(());
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
        Ok(())
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
                Arc::new(ParseSheetTool {
                    services: services.clone(),
                }),
                Arc::new(TrafficCampaignsTool { services }),
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
