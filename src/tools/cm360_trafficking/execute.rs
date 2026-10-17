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

//! Execution of reconciled CM360 operations and result summarisation (port of
//! the `_execute_*` / `_process_*` functions,
//! `_validate_and_resolve_ad_event_tags` and
//! `_build_trafficking_summary_response` in `helpers.py`).
//!
//! Operations are executed sequentially in the Python order: placement
//! patches, creative patches, event tag inserts/patches/updates, then ads
//! (whose event tag overrides are validated against the event tag results
//! first). Every executed operation yields an [`OperationResult`].
//!
//! Intentional deviation: Python raises `ValueError` for an unsupported ad
//! operation type (aborting the whole tool); here
//! [`process_ad_operations`] records a per-operation `ERROR` result
//! (`"Unsupported ad operation: {op}"`) instead, keeping the `Vec` return
//! type of the spec. Unsupported placement / creative operations still abort
//! with an error like Python.

use std::collections::{HashMap, HashSet};

use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tracing::{error, info, warn};

use super::client::Cm360Api;
use super::diff::{
    AD_INSERT_OPERATION, AD_PATCH_OPERATION, CREATIVE_PATCH_OPERATION, EVENT_TAG_INSERT_OPERATION,
    EVENT_TAG_PATCH_OPERATION, Operation, PLACEMENT_PATCH_OPERATION, value_to_string,
};
use super::sheet::EntityOutcomes;

/// Operation name for event tag updates (full replace).
pub const EVENT_TAG_UPDATE_OPERATION: &str = "dfareporting.eventTags.update";
/// Operation name for ad updates (full replace).
pub const AD_UPDATE_OPERATION: &str = "dfareporting.ads.update";

/// Result status for a successfully executed operation.
pub const STATUS_SUCCESS: &str = "SUCCESS";
/// Result status for a failed operation.
pub const STATUS_ERROR: &str = "ERROR";
/// Result status for a skipped operation.
pub const STATUS_SKIPPED: &str = "SKIPPED";
/// Summary status when some, but not all, operations failed.
pub const STATUS_PARTIAL_SUCCESS: &str = "PARTIAL_SUCCESS";

/// Outcome of executing one [`Operation`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResult {
    /// CM360 resource ID (the real ID returned by the API when available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Entity name.
    pub name: String,
    /// Operation name that was executed.
    pub operation: String,
    /// `SUCCESS`, `ERROR` or `SKIPPED`.
    pub status: String,
    /// Human-readable detail (always present for executed operations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl OperationResult {
    fn success(id: String, name: String, operation: &str, message: String) -> Self {
        Self {
            id: Some(id),
            name,
            operation: operation.to_string(),
            status: STATUS_SUCCESS.to_string(),
            message: Some(message),
        }
    }

    fn error(id: Option<String>, name: String, operation: &str, message: String) -> Self {
        Self {
            id,
            name,
            operation: operation.to_string(),
            status: STATUS_ERROR.to_string(),
            message: Some(message),
        }
    }
}

/// Operations split by entity type, in their original relative order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartitionedOperations {
    /// `dfareporting.placements.patch` operations.
    pub placement_ops: Vec<Operation>,
    /// `dfareporting.creatives.patch` operations.
    pub creative_ops: Vec<Operation>,
    /// `dfareporting.eventTags.insert|patch|update` operations.
    pub event_tag_ops: Vec<Operation>,
    /// `dfareporting.ads.insert|patch|update` operations.
    pub ad_ops: Vec<Operation>,
}

/// Splits operations into placement, creative, event tag and ad buckets
/// (operations with unknown names are dropped).
pub fn partition_operations(operations: &[Operation]) -> PartitionedOperations {
    let mut partitioned = PartitionedOperations::default();
    for op in operations {
        match op.operation.as_str() {
            PLACEMENT_PATCH_OPERATION => partitioned.placement_ops.push(op.clone()),
            CREATIVE_PATCH_OPERATION => partitioned.creative_ops.push(op.clone()),
            EVENT_TAG_INSERT_OPERATION | EVENT_TAG_PATCH_OPERATION | EVENT_TAG_UPDATE_OPERATION => {
                partitioned.event_tag_ops.push(op.clone());
            }
            AD_INSERT_OPERATION | AD_PATCH_OPERATION | AD_UPDATE_OPERATION => {
                partitioned.ad_ops.push(op.clone());
            }
            _ => {}
        }
    }
    partitioned
}

/// `str(res.get("id", fallback))`: the response's `id` when present (and not
/// `null`), otherwise `fallback`.
fn response_id(response: &Value, fallback: &str) -> String {
    match response.get("id") {
        None | Some(Value::Null) => fallback.to_string(),
        Some(id) => value_to_string(Some(id)),
    }
}

/// `str(op.get("id", payload.get("id", "")))`.
fn operation_id(op: &Operation, payload: &Value) -> String {
    op.id
        .clone()
        .unwrap_or_else(|| value_to_string(payload.get("id")))
}

/// Executes a single placement patch operation.
async fn execute_placement_operation(
    api: &dyn Cm360Api,
    profile_id: &str,
    op: &Operation,
) -> anyhow::Result<OperationResult> {
    let operation_type = op.operation.as_str();
    let placement_name = op.name.trim().to_string();
    let placement_id = op.id.as_deref().unwrap_or_default().trim().to_string();

    if operation_type != PLACEMENT_PATCH_OPERATION {
        bail!("Unsupported placement operation: {operation_type}");
    }

    info!("Executing {operation_type} for {placement_name} (ID: {placement_id})");
    match api
        .patch_placement(profile_id, &placement_id, &op.payload)
        .await
    {
        Err(api_err) => {
            error!("Failed to execute placement patch for {placement_name}: {api_err:#}");
            Ok(OperationResult::error(
                Some(placement_id),
                placement_name,
                operation_type,
                format!("Execution failed: {api_err}"),
            ))
        }
        Ok(res) => {
            let real_id = response_id(&res, &placement_id);
            info!("Successfully executed {operation_type} for {placement_name} (ID: {real_id})");
            let message =
                format!("Placement '{placement_name}' patched successfully with ID: {real_id}.");
            Ok(OperationResult::success(
                real_id,
                placement_name,
                operation_type,
                message,
            ))
        }
    }
}

/// Processes all placement patch operations and collects the results.
///
/// # Errors
///
/// `"Unsupported placement operation: {op}"` when an operation is not a
/// placement patch (mirrors the Python `ValueError`).
pub async fn process_placement_operations(
    api: &dyn Cm360Api,
    profile_id: &str,
    ops: &[Operation],
) -> anyhow::Result<Vec<OperationResult>> {
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        results.push(execute_placement_operation(api, profile_id, op).await?);
    }
    Ok(results)
}

/// Executes a single creative patch operation.
async fn execute_creative_operation(
    api: &dyn Cm360Api,
    profile_id: &str,
    op: &Operation,
) -> anyhow::Result<OperationResult> {
    let operation_type = op.operation.as_str();
    let creative_name = op.name.trim().to_string();
    let creative_id = op.id.as_deref().unwrap_or_default().trim().to_string();

    if operation_type != CREATIVE_PATCH_OPERATION {
        bail!("Unsupported creative operation: {operation_type}");
    }

    info!("Executing {operation_type} for {creative_name} (ID: {creative_id})");
    match api
        .patch_creative(profile_id, &creative_id, &op.payload)
        .await
    {
        Err(api_err) => {
            error!("Failed to execute creative patch for {creative_name}: {api_err:#}");
            Ok(OperationResult::error(
                Some(creative_id),
                creative_name,
                operation_type,
                format!("Execution failed: {api_err}"),
            ))
        }
        Ok(res) => {
            let real_id = response_id(&res, &creative_id);
            info!("Successfully executed {operation_type} for {creative_name} (ID: {real_id})");
            let message =
                format!("Creative '{creative_name}' patched successfully with ID: {real_id}.");
            Ok(OperationResult::success(
                real_id,
                creative_name,
                operation_type,
                message,
            ))
        }
    }
}

/// Processes all creative patch operations and collects the results.
///
/// # Errors
///
/// `"Unsupported creative operation: {op}"` when an operation is not a
/// creative patch (mirrors the Python `ValueError`).
pub async fn process_creative_operations(
    api: &dyn Cm360Api,
    profile_id: &str,
    ops: &[Operation],
) -> anyhow::Result<Vec<OperationResult>> {
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        results.push(execute_creative_operation(api, profile_id, op).await?);
    }
    Ok(results)
}

/// Executes a single event tag insert / patch / update operation and records
/// the resulting tag ID in `event_tag_mappings`.
async fn execute_event_tag_operation(
    api: &dyn Cm360Api,
    profile_id: &str,
    op: &Operation,
    event_tag_mappings: &mut HashMap<String, String>,
) -> anyhow::Result<OperationResult> {
    let payload = &op.payload;
    let tag_name = op.name.trim().to_string();
    let operation_type = op.operation.as_str();
    let tag_id = operation_id(op, payload);

    info!("Executing {operation_type} for {tag_name}");
    let res = match operation_type {
        EVENT_TAG_INSERT_OPERATION => api.insert_event_tag(profile_id, payload).await?,
        EVENT_TAG_PATCH_OPERATION => api.patch_event_tag(profile_id, &tag_id, payload).await?,
        EVENT_TAG_UPDATE_OPERATION => api.update_event_tag(profile_id, payload).await?,
        other => bail!("Unsupported event tag operation: {other}"),
    };

    let real_id = response_id(&res, &value_to_string(payload.get("id")));
    event_tag_mappings.insert(tag_name.clone(), real_id.clone());
    info!("Successfully executed {operation_type} for {tag_name} (ID: {real_id})");
    let message = format!(
        "Event tag '{tag_name}' operation {operation_type} executed successfully with ID: {real_id}."
    );
    Ok(OperationResult::success(
        real_id,
        tag_name,
        operation_type,
        message,
    ))
}

/// Processes all event tag operations, tracking the created / updated tag
/// IDs by name and the names of the tags that failed.
///
/// Failures (including unsupported operation types) never abort the run:
/// they are recorded as `ERROR` results whose message is the error text.
pub async fn process_event_tag_operations(
    api: &dyn Cm360Api,
    profile_id: &str,
    ops: &[Operation],
) -> (
    Vec<OperationResult>,
    HashMap<String, String>,
    HashSet<String>,
) {
    let mut results = Vec::with_capacity(ops.len());
    let mut created_event_tag_mappings: HashMap<String, String> = HashMap::new();
    let mut failed_event_tag_names: HashSet<String> = HashSet::new();

    for op in ops {
        let tag_name = op.name.trim().to_string();
        let operation_type = op.operation.as_str();
        match execute_event_tag_operation(api, profile_id, op, &mut created_event_tag_mappings)
            .await
        {
            Ok(record) => results.push(record),
            Err(api_err) => {
                error!(
                    "Failed to execute tag operation {operation_type} for {tag_name}: {api_err:#}"
                );
                failed_event_tag_names.insert(tag_name.clone());
                results.push(OperationResult::error(
                    None,
                    tag_name,
                    operation_type,
                    api_err.to_string(),
                ));
            }
        }
    }

    (results, created_event_tag_mappings, failed_event_tag_names)
}

/// Python `str.isdigit()` (ASCII digits only).
fn is_digit(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
}

/// Validates the ad's event tag dependencies and resolves override tag names
/// to real IDs (in place).
///
/// Returns `(false, missing_or_failed_tags)` when any override references a
/// tag that failed, or that is neither a known mapping nor already a numeric
/// ID; otherwise rewrites the mapped override IDs and returns `(true, [])`.
pub fn validate_and_resolve_ad_event_tags(
    payload: &mut Value,
    existing_event_tag_mappings: &HashMap<String, String>,
    failed_event_tag_names: &HashSet<String>,
) -> (bool, Vec<String>) {
    let override_ids: Vec<String> = payload
        .get("eventTagOverrides")
        .and_then(Value::as_array)
        .map(|overrides| {
            overrides
                .iter()
                .map(|o| value_to_string(o.get("id")).trim().to_string())
                .collect()
        })
        .unwrap_or_default();

    let missing_or_failed_tags: Vec<String> = override_ids
        .iter()
        .filter(|target| {
            failed_event_tag_names.contains(*target)
                || (!existing_event_tag_mappings.contains_key(*target) && !is_digit(target))
        })
        .cloned()
        .collect();
    if !missing_or_failed_tags.is_empty() {
        return (false, missing_or_failed_tags);
    }

    if let Some(overrides) = payload
        .get_mut("eventTagOverrides")
        .and_then(Value::as_array_mut)
    {
        for override_entry in overrides.iter_mut() {
            let target_tag = value_to_string(override_entry.get("id")).trim().to_string();
            if let Some(real_id) = existing_event_tag_mappings.get(&target_tag)
                && let Some(obj) = override_entry.as_object_mut()
            {
                obj.insert("id".into(), json!(real_id));
            }
        }
    }

    (true, Vec::new())
}

/// Validates dependencies and executes a single ad operation.
async fn execute_ad_operation(
    api: &dyn Cm360Api,
    profile_id: &str,
    op: &Operation,
    event_tag_mappings: &HashMap<String, String>,
    failed_event_tag_names: &HashSet<String>,
) -> OperationResult {
    let operation_type = op.operation.as_str();
    let mut payload = op.payload.clone();
    let ad_name = op.name.trim().to_string();

    let (is_valid, missing_tags) = validate_and_resolve_ad_event_tags(
        &mut payload,
        event_tag_mappings,
        failed_event_tag_names,
    );
    if !is_valid {
        warn!(
            "Skipping ad '{ad_name}' because associated event tag(s) failed or are missing: \
             {missing_tags:?}"
        );
        return OperationResult::error(
            None,
            ad_name,
            operation_type,
            format!(
                "Ad operation aborted: Dependent event tag(s) failed or missing: {}.",
                missing_tags.join(", ")
            ),
        );
    }

    let ad_id = operation_id(op, &payload);
    info!("Executing {operation_type} for {ad_name}");
    let call = match operation_type {
        AD_INSERT_OPERATION => api.insert_ad(profile_id, &payload).await,
        AD_PATCH_OPERATION => api.patch_ad(profile_id, &ad_id, &payload).await,
        AD_UPDATE_OPERATION => api.update_ad(profile_id, &payload).await,
        other => {
            error!("Unsupported ad operation: {other}");
            return OperationResult::error(
                None,
                ad_name,
                operation_type,
                format!("Unsupported ad operation: {other}"),
            );
        }
    };

    match call {
        Err(api_err) => {
            error!("Failed to execute ad operation {operation_type} for {ad_name}: {api_err:#}");
            OperationResult::error(
                Some(ad_id),
                ad_name,
                operation_type,
                format!("Execution failed: {api_err}"),
            )
        }
        Ok(res) => {
            let real_ad_id = response_id(&res, &ad_id);
            info!("Successfully executed {operation_type} for {ad_name} (ID: {real_ad_id})");
            let message = format!(
                "Ad operation {operation_type} executed successfully with ID: {real_ad_id}."
            );
            OperationResult::success(real_ad_id, ad_name, operation_type, message)
        }
    }
}

/// Processes all ad operations (validating event tag dependencies first) and
/// collects the results.
pub async fn process_ad_operations(
    api: &dyn Cm360Api,
    profile_id: &str,
    ops: &[Operation],
    event_tag_mappings: &HashMap<String, String>,
    failed_event_tag_names: &HashSet<String>,
) -> Vec<OperationResult> {
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        results.push(
            execute_ad_operation(
                api,
                profile_id,
                op,
                event_tag_mappings,
                failed_event_tag_names,
            )
            .await,
        );
    }
    results
}

/// Builds the pretty-printed (2-space indent) JSON summary of a trafficking
/// run, with the same keys and key order as the Python implementation:
/// `status`, `message`, optional `profile_id` / `advertiser_id` /
/// `campaign_id` / `campaign_name`, then `results` (each with optional `id`,
/// `name`, `operation`, `status` and — for errors only — `message`).
pub fn build_trafficking_summary_response(
    results: &[OperationResult],
    total_operations_count: usize,
    profile_id: Option<&str>,
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
    campaign_name: Option<&str>,
) -> String {
    let count = |status: &str| results.iter().filter(|r| r.status == status).count();
    let success_count = count(STATUS_SUCCESS);
    let skipped_count = count(STATUS_SKIPPED);
    let error_count = count(STATUS_ERROR);

    let clean_results: Vec<Value> = results
        .iter()
        .map(|r| {
            let mut record = Map::new();
            if let Some(id) = r.id.as_deref().filter(|id| !id.is_empty()) {
                record.insert("id".into(), json!(id));
            }
            record.insert("name".into(), json!(r.name));
            record.insert("operation".into(), json!(r.operation));
            record.insert("status".into(), json!(r.status));
            if r.status == STATUS_ERROR
                && let Some(message) = r.message.as_deref().filter(|m| !m.is_empty())
            {
                record.insert("message".into(), json!(message));
            }
            Value::Object(record)
        })
        .collect();

    let status = if error_count == 0 {
        STATUS_SUCCESS
    } else if success_count + skipped_count == 0 {
        STATUS_ERROR
    } else {
        STATUS_PARTIAL_SUCCESS
    };
    let message = if error_count == 0 {
        format!(
            "Successfully executed trafficking for {total_operations_count} operation(s): \
             {success_count} processed, {skipped_count} skipped."
        )
    } else {
        format!(
            "Trafficking execution encountered {error_count} error(s) ({success_count} processed, \
             {skipped_count} skipped). The failed rows keep their New/Update status in the \
             trafficking sheet: run parse_sheet_tool again and re-approve to reprocess them."
        )
    };

    let mut response = Map::new();
    response.insert("status".into(), json!(status));
    response.insert("message".into(), json!(message));
    for (key, value) in [
        ("profile_id", profile_id),
        ("advertiser_id", advertiser_id),
        ("campaign_id", campaign_id),
        ("campaign_name", campaign_name),
    ] {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            response.insert(key.into(), json!(value));
        }
    }
    response.insert("results".into(), Value::Array(clean_results));

    serde_json::to_string_pretty(&Value::Object(response)).unwrap_or_default()
}

/// Names (trimmed) of the successful ad operations and of the successful
/// placement operations.
pub fn successful_entity_names(results: &[OperationResult]) -> (HashSet<String>, HashSet<String>) {
    let outcomes = entity_outcomes(results);
    (outcomes.successful_ads, outcomes.successful_placements)
}

/// Per-entity success / failure sets (trimmed names) of the ad and placement
/// operations — the input to
/// [`apply_trafficked_status`](super::sheet::apply_trafficked_status).
///
/// `SKIPPED` results count as neither success nor failure.
pub fn entity_outcomes(results: &[OperationResult]) -> EntityOutcomes {
    let names_for = |prefix: &str, status: &str| -> HashSet<String> {
        results
            .iter()
            .filter(|r| r.status == status && r.operation.starts_with(prefix))
            .map(|r| r.name.trim().to_string())
            .collect()
    };
    EntityOutcomes {
        successful_ads: names_for("dfareporting.ads.", STATUS_SUCCESS),
        successful_placements: names_for("dfareporting.placements.", STATUS_SUCCESS),
        failed_ads: names_for("dfareporting.ads.", STATUS_ERROR),
        failed_placements: names_for("dfareporting.placements.", STATUS_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::anyhow;

    use super::super::client::test_support::MockApi;
    use super::*;

    fn op(operation: &str, id: Option<&str>, name: &str, payload: Value) -> Operation {
        Operation {
            operation: operation.to_string(),
            id: id.map(str::to_string),
            name: name.to_string(),
            diff_fields: None,
            payload,
        }
    }

    #[test]
    fn test_partition_operations() {
        let ops = vec![
            op(AD_INSERT_OPERATION, None, "A1", json!({})),
            op(PLACEMENT_PATCH_OPERATION, Some("p"), "P", json!({})),
            op(EVENT_TAG_UPDATE_OPERATION, None, "T2", json!({})),
            op(CREATIVE_PATCH_OPERATION, Some("c"), "C", json!({})),
            op(EVENT_TAG_INSERT_OPERATION, None, "T1", json!({})),
            op(AD_PATCH_OPERATION, Some("a"), "A2", json!({})),
            op(AD_UPDATE_OPERATION, None, "A3", json!({})),
            op(EVENT_TAG_PATCH_OPERATION, Some("t"), "T3", json!({})),
            op("dfareporting.creatives.insert", None, "ignored", json!({})),
        ];
        let partitioned = partition_operations(&ops);
        let names = |ops: &[Operation]| ops.iter().map(|o| o.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&partitioned.placement_ops), vec!["P"]);
        assert_eq!(names(&partitioned.creative_ops), vec!["C"]);
        assert_eq!(names(&partitioned.event_tag_ops), vec!["T2", "T1", "T3"]);
        assert_eq!(names(&partitioned.ad_ops), vec!["A1", "A2", "A3"]);
        assert_eq!(partition_operations(&[]), PartitionedOperations::default());
    }

    #[tokio::test]
    async fn placement_operations_success_error_and_unsupported() {
        let api = MockApi::new().with_responder(|call| {
            if call.id.as_deref() == Some("bad") {
                Err(anyhow!("boom"))
            } else {
                Ok(json!({"id": 777}))
            }
        });
        let ops = vec![
            op(
                PLACEMENT_PATCH_OPERATION,
                Some(" p1 "),
                " Placement One ",
                json!({"activeStatus": "X"}),
            ),
            op(PLACEMENT_PATCH_OPERATION, Some("bad"), "Broken", json!({})),
        ];
        let results = process_placement_operations(&api, "123", &ops)
            .await
            .unwrap();
        assert_eq!(
            results,
            vec![
                OperationResult {
                    id: Some("777".into()),
                    name: "Placement One".into(),
                    operation: PLACEMENT_PATCH_OPERATION.into(),
                    status: "SUCCESS".into(),
                    message: Some(
                        "Placement 'Placement One' patched successfully with ID: 777.".into()
                    ),
                },
                OperationResult {
                    id: Some("bad".into()),
                    name: "Broken".into(),
                    operation: PLACEMENT_PATCH_OPERATION.into(),
                    status: "ERROR".into(),
                    message: Some("Execution failed: boom".into()),
                },
            ]
        );
        let calls = api.mutation_calls();
        assert_eq!(calls[0].method, "patch_placement");
        assert_eq!(calls[0].profile_id, "123");
        assert_eq!(calls[0].id.as_deref(), Some("p1"));
        assert_eq!(calls[0].body, json!({"activeStatus": "X"}));

        // Missing ID / response without id → falls back to the op's ID.
        let api = MockApi::new().with_responder(|_| Ok(json!({})));
        let results = process_placement_operations(
            &api,
            "123",
            &[op(PLACEMENT_PATCH_OPERATION, None, "P", json!({}))],
        )
        .await
        .unwrap();
        assert_eq!(results[0].id.as_deref(), Some(""));
        assert_eq!(results[0].status, "SUCCESS");

        let err = process_placement_operations(
            &api,
            "123",
            &[op("dfareporting.placements.insert", None, "P", json!({}))],
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Unsupported placement operation: dfareporting.placements.insert"
        );
    }

    #[tokio::test]
    async fn creative_operations_success_error_and_unsupported() {
        let api = MockApi::new().with_responder(|call| {
            if call.id.as_deref() == Some("bad") {
                Err(anyhow!("denied"))
            } else {
                Ok(json!({"id": "c1"}))
            }
        });
        let ops = vec![
            op(
                CREATIVE_PATCH_OPERATION,
                Some("c1"),
                "Creative One",
                json!({"size": {"width": 1, "height": 2}}),
            ),
            op(CREATIVE_PATCH_OPERATION, Some("bad"), "Broken", json!({})),
        ];
        let results = process_creative_operations(&api, "123", &ops)
            .await
            .unwrap();
        assert_eq!(results[0].status, "SUCCESS");
        assert_eq!(results[0].id.as_deref(), Some("c1"));
        assert_eq!(
            results[0].message.as_deref(),
            Some("Creative 'Creative One' patched successfully with ID: c1.")
        );
        assert_eq!(results[1].status, "ERROR");
        assert_eq!(results[1].id.as_deref(), Some("bad"));
        assert_eq!(
            results[1].message.as_deref(),
            Some("Execution failed: denied")
        );
        assert_eq!(api.mutation_calls()[0].method, "patch_creative");

        let err = process_creative_operations(
            &api,
            "123",
            &[op(AD_INSERT_OPERATION, None, "X", json!({}))],
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Unsupported creative operation: dfareporting.ads.insert"
        );
    }

    #[tokio::test]
    async fn test_execute_event_tag_patch_operation() {
        let api =
            MockApi::new().with_responder(|_| Ok(json!({"id": "tag_123", "name": "Test Tag"})));
        let ops = vec![op(
            EVENT_TAG_PATCH_OPERATION,
            Some("tag_123"),
            "Test Tag",
            json!({"url": "https://example.com/new"}),
        )];
        let (results, mappings, failed) = process_event_tag_operations(&api, "12345", &ops).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, "SUCCESS");
        assert_eq!(results[0].id.as_deref(), Some("tag_123"));
        assert_eq!(
            results[0].message.as_deref(),
            Some(
                "Event tag 'Test Tag' operation dfareporting.eventTags.patch executed successfully with ID: tag_123."
            )
        );
        assert_eq!(mappings["Test Tag"], "tag_123");
        assert!(failed.is_empty());
        let calls = api.mutation_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "patch_event_tag");
        assert_eq!(calls[0].profile_id, "12345");
        assert_eq!(calls[0].id.as_deref(), Some("tag_123"));
        assert_eq!(calls[0].body, json!({"url": "https://example.com/new"}));
    }

    #[tokio::test]
    async fn event_tag_operations_insert_update_failure_and_unsupported() {
        let api = MockApi::new().with_responder(|call| match call.method {
            "insert_event_tag" => Ok(json!({"id": "new_1"})),
            "update_event_tag" => Ok(json!({})),
            _ => Err(anyhow!("quota exceeded")),
        });
        let ops = vec![
            op(
                EVENT_TAG_INSERT_OPERATION,
                None,
                " New Tag ",
                json!({"name": "New Tag"}),
            ),
            op(
                EVENT_TAG_UPDATE_OPERATION,
                None,
                "Upd Tag",
                json!({"id": "u9", "name": "Upd Tag"}),
            ),
            op(
                EVENT_TAG_PATCH_OPERATION,
                Some("p1"),
                "Patch Tag",
                json!({}),
            ),
            op("dfareporting.eventTags.delete", None, "Del Tag", json!({})),
        ];
        let (results, mappings, failed) = process_event_tag_operations(&api, "1", &ops).await;
        assert_eq!(
            results
                .iter()
                .map(|r| (r.name.as_str(), r.status.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("New Tag", "SUCCESS"),
                ("Upd Tag", "SUCCESS"),
                ("Patch Tag", "ERROR"),
                ("Del Tag", "ERROR"),
            ]
        );
        assert_eq!(results[0].id.as_deref(), Some("new_1"));
        // Update without an id in the response falls back to the payload id.
        assert_eq!(results[1].id.as_deref(), Some("u9"));
        assert_eq!(results[2].id, None);
        assert_eq!(results[2].message.as_deref(), Some("quota exceeded"));
        assert_eq!(
            results[3].message.as_deref(),
            Some("Unsupported event tag operation: dfareporting.eventTags.delete")
        );
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings["New Tag"], "new_1");
        assert_eq!(mappings["Upd Tag"], "u9");
        assert_eq!(
            failed,
            ["Patch Tag".to_string(), "Del Tag".to_string()]
                .into_iter()
                .collect()
        );
        let methods: Vec<&str> = api.mutation_calls().iter().map(|c| c.method).collect();
        assert_eq!(
            methods,
            vec!["insert_event_tag", "update_event_tag", "patch_event_tag"]
        );
    }

    #[test]
    fn test_validate_and_resolve_ad_event_tags() {
        let mappings: HashMap<String, String> = [("Tag A".to_string(), "100".to_string())]
            .into_iter()
            .collect();
        let failed: HashSet<String> = ["Tag F".to_string()].into_iter().collect();

        let mut payload = json!({"name": "no overrides"});
        assert_eq!(
            validate_and_resolve_ad_event_tags(&mut payload, &mappings, &failed),
            (true, vec![])
        );

        let mut payload = json!({"eventTagOverrides": [
            {"id": " Tag A ", "enabled": true},
            {"id": "12345", "enabled": true},
        ]});
        assert_eq!(
            validate_and_resolve_ad_event_tags(&mut payload, &mappings, &failed),
            (true, vec![])
        );
        assert_eq!(
            payload["eventTagOverrides"],
            json!([{"id": "100", "enabled": true}, {"id": "12345", "enabled": true}])
        );

        let mut payload = json!({"eventTagOverrides": [
            {"id": "Tag A"},
            {"id": "Tag F"},
            {"id": "Unknown"},
            {"id": ""},
        ]});
        let (ok, missing) = validate_and_resolve_ad_event_tags(&mut payload, &mappings, &failed);
        assert!(!ok);
        assert_eq!(missing, vec!["Tag F", "Unknown", ""]);
        // Payload untouched on failure.
        assert_eq!(payload["eventTagOverrides"][0]["id"], "Tag A");

        // A failed tag is rejected even when it has a mapping.
        let failed_mapped: HashSet<String> = ["Tag A".to_string()].into_iter().collect();
        let mut payload = json!({"eventTagOverrides": [{"id": "Tag A"}]});
        assert_eq!(
            validate_and_resolve_ad_event_tags(&mut payload, &mappings, &failed_mapped),
            (false, vec!["Tag A".to_string()])
        );
    }

    #[tokio::test]
    async fn ad_operations_cover_all_paths() {
        let api = MockApi::new().with_responder(|call| match (call.method, call.id.as_deref()) {
            ("insert_ad", _) => Ok(json!({"id": "ad_new"})),
            ("patch_ad", Some("bad")) => Err(anyhow!("backend error")),
            ("patch_ad", _) => Ok(json!({"id": "ad_1"})),
            ("update_ad", _) => Ok(json!({})),
            _ => Err(anyhow!("unexpected")),
        });
        let mappings: HashMap<String, String> = [("Tag A".to_string(), "tag_a_id".to_string())]
            .into_iter()
            .collect();
        let failed: HashSet<String> = ["Tag F".to_string()].into_iter().collect();

        let ops = vec![
            op(
                AD_INSERT_OPERATION,
                None,
                " New Ad ",
                json!({"name": "New Ad", "eventTagOverrides": [{"id": "Tag A", "enabled": true}]}),
            ),
            op(
                AD_PATCH_OPERATION,
                Some("ad_1"),
                "Patched Ad",
                json!({"startTime": "x"}),
            ),
            op(
                AD_UPDATE_OPERATION,
                None,
                "Updated Ad",
                json!({"id": "ad_2", "name": "Updated Ad"}),
            ),
            op(
                AD_INSERT_OPERATION,
                None,
                "Blocked Ad",
                json!({"eventTagOverrides": [{"id": "Tag F"}, {"id": "Nope"}]}),
            ),
            op(AD_PATCH_OPERATION, Some("bad"), "Failing Ad", json!({})),
            op("dfareporting.ads.delete", Some("x"), "Weird Ad", json!({})),
        ];
        let results = process_ad_operations(&api, "77", &ops, &mappings, &failed).await;
        assert_eq!(
            results,
            vec![
                OperationResult {
                    id: Some("ad_new".into()),
                    name: "New Ad".into(),
                    operation: AD_INSERT_OPERATION.into(),
                    status: "SUCCESS".into(),
                    message: Some(
                        "Ad operation dfareporting.ads.insert executed successfully with ID: ad_new.".into()
                    ),
                },
                OperationResult {
                    id: Some("ad_1".into()),
                    name: "Patched Ad".into(),
                    operation: AD_PATCH_OPERATION.into(),
                    status: "SUCCESS".into(),
                    message: Some(
                        "Ad operation dfareporting.ads.patch executed successfully with ID: ad_1.".into()
                    ),
                },
                OperationResult {
                    id: Some("ad_2".into()),
                    name: "Updated Ad".into(),
                    operation: AD_UPDATE_OPERATION.into(),
                    status: "SUCCESS".into(),
                    message: Some(
                        "Ad operation dfareporting.ads.update executed successfully with ID: ad_2.".into()
                    ),
                },
                OperationResult {
                    id: None,
                    name: "Blocked Ad".into(),
                    operation: AD_INSERT_OPERATION.into(),
                    status: "ERROR".into(),
                    message: Some(
                        "Ad operation aborted: Dependent event tag(s) failed or missing: Tag F, Nope.".into()
                    ),
                },
                OperationResult {
                    id: Some("bad".into()),
                    name: "Failing Ad".into(),
                    operation: AD_PATCH_OPERATION.into(),
                    status: "ERROR".into(),
                    message: Some("Execution failed: backend error".into()),
                },
                OperationResult {
                    id: None,
                    name: "Weird Ad".into(),
                    operation: "dfareporting.ads.delete".into(),
                    status: "ERROR".into(),
                    message: Some("Unsupported ad operation: dfareporting.ads.delete".into()),
                },
            ]
        );
        let calls = api.mutation_calls();
        assert_eq!(
            calls
                .iter()
                .map(|c| (c.method, c.id.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("insert_ad", None),
                ("patch_ad", Some("ad_1")),
                ("update_ad", None),
                ("patch_ad", Some("bad")),
            ]
        );
        assert_eq!(calls[0].profile_id, "77");
        // The override name was resolved to the real ID in the request body.
        assert_eq!(
            calls[0].body,
            json!({"name": "New Ad", "eventTagOverrides": [{"id": "tag_a_id", "enabled": true}]})
        );
        // The original operation payload is untouched.
        assert_eq!(ops[0].payload["eventTagOverrides"][0]["id"], "Tag A");
    }

    #[test]
    fn summary_response_success() {
        let results = vec![
            OperationResult {
                id: Some("p1".into()),
                name: "P".into(),
                operation: PLACEMENT_PATCH_OPERATION.into(),
                status: "SUCCESS".into(),
                message: Some("ignored for success".into()),
            },
            OperationResult {
                id: None,
                name: "S".into(),
                operation: AD_INSERT_OPERATION.into(),
                status: "SKIPPED".into(),
                message: None,
            },
        ];
        let summary = build_trafficking_summary_response(
            &results,
            3,
            Some("123"),
            Some("456"),
            Some("789"),
            Some("Camp"),
        );
        let expected = r#"{
  "status": "SUCCESS",
  "message": "Successfully executed trafficking for 3 operation(s): 1 processed, 1 skipped.",
  "profile_id": "123",
  "advertiser_id": "456",
  "campaign_id": "789",
  "campaign_name": "Camp",
  "results": [
    {
      "id": "p1",
      "name": "P",
      "operation": "dfareporting.placements.patch",
      "status": "SUCCESS"
    },
    {
      "name": "S",
      "operation": "dfareporting.ads.insert",
      "status": "SKIPPED"
    }
  ]
}"#;
        assert_eq!(summary, expected);
    }

    #[test]
    fn summary_response_partial_and_error() {
        let ok = OperationResult {
            id: Some("a1".into()),
            name: "A".into(),
            operation: AD_INSERT_OPERATION.into(),
            status: "SUCCESS".into(),
            message: None,
        };
        let err = OperationResult {
            id: Some(String::new()),
            name: "B".into(),
            operation: AD_INSERT_OPERATION.into(),
            status: "ERROR".into(),
            message: Some("Execution failed: boom".into()),
        };
        let partial: Value = serde_json::from_str(&build_trafficking_summary_response(
            &[ok.clone(), err.clone()],
            2,
            Some("1"),
            None,
            Some(""),
            None,
        ))
        .unwrap();
        assert_eq!(partial["status"], "PARTIAL_SUCCESS");
        assert_eq!(
            partial["message"],
            "Trafficking execution encountered 1 error(s) (1 processed, 0 skipped). The failed rows keep their New/Update status in the trafficking sheet: run parse_sheet_tool again and re-approve to reprocess them."
        );
        assert_eq!(partial["profile_id"], "1");
        assert!(partial.get("advertiser_id").is_none());
        assert!(partial.get("campaign_id").is_none());
        assert!(partial.get("campaign_name").is_none());
        // Empty IDs are omitted; error messages are kept.
        assert_eq!(
            partial["results"][1],
            json!({
                "name": "B",
                "operation": "dfareporting.ads.insert",
                "status": "ERROR",
                "message": "Execution failed: boom",
            })
        );
        assert_eq!(
            partial.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["status", "message", "profile_id", "results"]
        );

        let all_failed: Value = serde_json::from_str(&build_trafficking_summary_response(
            &[err],
            1,
            None,
            None,
            None,
            None,
        ))
        .unwrap();
        assert_eq!(all_failed["status"], "ERROR");

        let empty: Value = serde_json::from_str(&build_trafficking_summary_response(
            &[],
            0,
            None,
            None,
            None,
            None,
        ))
        .unwrap();
        assert_eq!(empty["status"], "SUCCESS");
        assert_eq!(
            empty["message"],
            "Successfully executed trafficking for 0 operation(s): 0 processed, 0 skipped."
        );
        assert_eq!(empty["results"], json!([]));
    }

    #[test]
    fn test_successful_entity_names() {
        let results = vec![
            OperationResult {
                id: None,
                name: " Ad One ".into(),
                operation: AD_INSERT_OPERATION.into(),
                status: "SUCCESS".into(),
                message: None,
            },
            OperationResult {
                id: None,
                name: "Ad Two".into(),
                operation: AD_PATCH_OPERATION.into(),
                status: "ERROR".into(),
                message: None,
            },
            OperationResult {
                id: None,
                name: "Ad Three".into(),
                operation: AD_UPDATE_OPERATION.into(),
                status: "SUCCESS".into(),
                message: None,
            },
            OperationResult {
                id: None,
                name: "Placement One".into(),
                operation: PLACEMENT_PATCH_OPERATION.into(),
                status: "SUCCESS".into(),
                message: None,
            },
            OperationResult {
                id: None,
                name: "Tag".into(),
                operation: EVENT_TAG_INSERT_OPERATION.into(),
                status: "SUCCESS".into(),
                message: None,
            },
        ];
        let (ads, placements) = successful_entity_names(&results);
        assert_eq!(
            ads,
            ["Ad One".to_string(), "Ad Three".to_string()]
                .into_iter()
                .collect()
        );
        assert_eq!(
            placements,
            ["Placement One".to_string()].into_iter().collect()
        );

        let mut results = results;
        results.push(OperationResult {
            id: None,
            name: " Placement Two ".into(),
            operation: PLACEMENT_PATCH_OPERATION.into(),
            status: "ERROR".into(),
            message: None,
        });
        results.push(OperationResult {
            id: None,
            name: "Skipped Ad".into(),
            operation: AD_INSERT_OPERATION.into(),
            status: "SKIPPED".into(),
            message: None,
        });
        let outcomes = entity_outcomes(&results);
        assert_eq!(outcomes.successful_ads, ads);
        assert_eq!(outcomes.successful_placements, placements);
        assert_eq!(
            outcomes.failed_ads,
            ["Ad Two".to_string()].into_iter().collect()
        );
        assert_eq!(
            outcomes.failed_placements,
            ["Placement Two".to_string()].into_iter().collect()
        );
        assert!(outcomes.has_successes());
    }

    #[test]
    fn operation_result_serialisation() {
        let result = OperationResult {
            id: None,
            name: "A".into(),
            operation: AD_INSERT_OPERATION.into(),
            status: "ERROR".into(),
            message: Some("m".into()),
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            json!({"name": "A", "operation": "dfareporting.ads.insert", "status": "ERROR", "message": "m"})
        );
        let parsed: OperationResult =
            serde_json::from_value(json!({"name": "A", "operation": "x", "status": "SUCCESS"}))
                .unwrap();
        assert_eq!(parsed.id, None);
        assert_eq!(parsed.message, None);
    }
}
