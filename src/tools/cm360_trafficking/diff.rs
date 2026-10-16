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

//! Reconciliation of sheet entities against live CM360 state (port of the
//! `_diff_*`, `_resolve_*_ids`, `_extract_assigned_placement_ids`,
//! `_reconcile_missing_ad_placements` and `_resolve_and_build_operations`
//! helpers in `helpers.py`).
//!
//! Like the Python code, the resolution steps mutate the grouped `ads` map in
//! place (placement / creative / event tag names are rewritten to real CM360
//! IDs) and operations are built from cloned payloads.
//!
//! Intentional deviation: when comparing creative rotation weights the Python
//! code calls `int()` on both sides and would raise `TypeError` if CM360 did
//! not return a `weight`; here a missing weight on either side simply skips
//! the weight comparison.

use std::collections::{HashMap, HashSet};

use anyhow::bail;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tracing::info;

use super::client::Cm360Api;
use super::grouping::EntityMap;
use super::sheet::{format_date, normalize_iso_datetime};

/// A single CM360 API operation produced by the reconciliation step.
///
/// Serialises to the same JSON shape as the Python dictionaries:
/// `{"operation", "id"?, "name", "diff_fields"?, "payload"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// Fully qualified operation name, e.g. `dfareporting.ads.insert`.
    pub operation: String,
    /// Existing CM360 resource ID (patch operations only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Entity name (the sheet's matching key).
    #[serde(default)]
    pub name: String,
    /// Names of the fields that differ from CM360 (patch operations only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_fields: Option<Vec<String>>,
    /// Request body for the operation.
    pub payload: Value,
}

/// Clean (trimmed) entity name → existing CM360 resource.
pub type ResourceMap = IndexMap<String, Value>;

/// Operation name for placement patches.
pub const PLACEMENT_PATCH_OPERATION: &str = "dfareporting.placements.patch";
/// Operation name for creative patches.
pub const CREATIVE_PATCH_OPERATION: &str = "dfareporting.creatives.patch";
/// Operation name for event tag insertions.
pub const EVENT_TAG_INSERT_OPERATION: &str = "dfareporting.eventTags.insert";
/// Operation name for event tag patches.
pub const EVENT_TAG_PATCH_OPERATION: &str = "dfareporting.eventTags.patch";
/// Operation name for ad insertions.
pub const AD_INSERT_OPERATION: &str = "dfareporting.ads.insert";
/// Operation name for ad patches.
pub const AD_PATCH_OPERATION: &str = "dfareporting.ads.patch";

/// Python truthiness for a JSON value (`None`/missing → false).
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// `str(value)` for the JSON values we deal with: strings verbatim, numbers
/// / booleans rendered, `null` / missing → `""`.
pub(crate) fn value_to_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// `Some(str(value))` when present and not `null`.
fn value_to_opt_string(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

/// `int(value)`: numbers truncated, numeric strings parsed; anything else is
/// `None`.
fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>()
                .ok()
                .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
        }
        _ => None,
    }
}

/// The object stored under `key`, or an empty object view.
fn object_at<'a>(value: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    value.get(key).and_then(Value::as_object)
}

/// The array stored under `key`, or an empty slice.
fn array_at<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// `str(obj.get(key, "")).strip()`.
fn stripped_str(obj: &Value, key: &str) -> String {
    value_to_string(obj.get(key)).trim().to_string()
}

/// `obj.get("clickThroughUrl", {}).get("customClickThroughUrl", "").strip()`.
fn custom_click_through_url(obj: &Value) -> String {
    object_at(obj, "clickThroughUrl")
        .map(|url| value_to_string(url.get("customClickThroughUrl")))
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Width/height of a `size` object as integers (`0` when missing).
fn size_of(entity: &Value) -> (i64, i64) {
    let size = object_at(entity, "size");
    let dim = |key: &str| size.and_then(|s| value_to_i64(s.get(key))).unwrap_or(0);
    (dim("width"), dim("height"))
}

/// Diffs the `size` of a sheet entity against the CM360 entity, recording a
/// `size` patch when the sheet size is known and differs.
fn diff_size(
    sheet: &Value,
    cm: &Value,
    patch: &mut Map<String, Value>,
    diff_fields: &mut Vec<String>,
) {
    let (s_w, s_h) = size_of(sheet);
    let (cm_w, cm_h) = size_of(cm);
    if (s_w, s_h) != (0, 0) && (s_w != cm_w || s_h != cm_h) {
        patch.insert("size".into(), json!({"width": s_w, "height": s_h}));
        diff_fields.push("size".into());
    }
}

/// Diffs sheet placement fields against an existing CM360 placement.
///
/// Compares only `pricingSchedule.startDate` / `endDate` (re-aligning the
/// `pricingPeriods` when a date changes), `activeStatus` and `size`. The
/// `name` is never diffed (it is the matching key).
pub fn diff_placement(sheet_placement: &Value, cm_placement: &Value) -> (Value, Vec<String>) {
    let mut patch_payload = Map::new();
    let mut diff_fields: Vec<String> = Vec::new();

    // 1. Diff pricingSchedule dates.
    let empty = Map::new();
    let sheet_pricing = object_at(sheet_placement, "pricingSchedule").unwrap_or(&empty);
    let cm_pricing = object_at(cm_placement, "pricingSchedule").unwrap_or(&empty);

    let date_of = |pricing: &Map<String, Value>, key: &str| {
        format_date(value_to_opt_string(pricing.get(key)).as_deref())
    };
    let sheet_start = date_of(sheet_pricing, "startDate");
    let cm_start = date_of(cm_pricing, "startDate");
    let sheet_end = date_of(sheet_pricing, "endDate");
    let cm_end = date_of(cm_pricing, "endDate");

    let mut pricing_patch = Map::new();
    let mut dates_changed = false;
    if let Some(start) = sheet_start.as_deref().filter(|s| !s.is_empty())
        && sheet_start != cm_start
    {
        pricing_patch.insert("startDate".into(), json!(start));
        diff_fields.push("pricingSchedule.startDate".into());
        dates_changed = true;
    }
    if let Some(end) = sheet_end.as_deref().filter(|s| !s.is_empty())
        && sheet_end != cm_end
    {
        pricing_patch.insert("endDate".into(), json!(end));
        diff_fields.push("pricingSchedule.endDate".into());
        dates_changed = true;
    }

    if dates_changed {
        let new_start = sheet_start.filter(|s| !s.is_empty()).or(cm_start);
        let new_end = sheet_end.filter(|s| !s.is_empty()).or(cm_end);
        let cm_periods = cm_pricing
            .get("pricingPeriods")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let updated_periods: Vec<Value> = if cm_periods.is_empty() {
            vec![json!({"startDate": new_start, "endDate": new_end})]
        } else {
            cm_periods
                .iter()
                .map(|period| {
                    let mut p = period.as_object().cloned().unwrap_or_default();
                    p.insert("startDate".into(), json!(new_start));
                    p.insert("endDate".into(), json!(new_end));
                    Value::Object(p)
                })
                .collect()
        };
        pricing_patch.insert("pricingPeriods".into(), Value::Array(updated_periods));
    }

    if !pricing_patch.is_empty() {
        patch_payload.insert("pricingSchedule".into(), Value::Object(pricing_patch));
    }

    // 2. Diff activeStatus.
    let sheet_status = stripped_str(sheet_placement, "activeStatus");
    let cm_status = stripped_str(cm_placement, "activeStatus");
    if !sheet_status.is_empty() && sheet_status != cm_status {
        patch_payload.insert("activeStatus".into(), json!(sheet_status));
        diff_fields.push("activeStatus".into());
    }

    // 3. Diff size.
    diff_size(
        sheet_placement,
        cm_placement,
        &mut patch_payload,
        &mut diff_fields,
    );

    (Value::Object(patch_payload), diff_fields)
}

/// Diffs sheet creative fields (`size` only) against an existing CM360
/// creative. The `name` is never diffed.
pub fn diff_creative(sheet_creative: &Value, cm_creative: &Value) -> (Value, Vec<String>) {
    let mut patch_payload = Map::new();
    let mut diff_fields: Vec<String> = Vec::new();
    diff_size(
        sheet_creative,
        cm_creative,
        &mut patch_payload,
        &mut diff_fields,
    );
    (Value::Object(patch_payload), diff_fields)
}

/// Set of stripped, truthy `placementId` values of a `placementAssignments`
/// list.
fn placement_id_set(ad: &Value) -> HashSet<String> {
    array_at(ad, "placementAssignments")
        .iter()
        .filter(|pa| truthy(pa.get("placementId")))
        .map(|pa| stripped_str(pa, "placementId"))
        .collect()
}

/// `creativeId` → assignment map of a creative rotation (later duplicates
/// overwrite earlier ones, like a dict comprehension).
fn creative_assignment_map(ad: &Value) -> IndexMap<String, &Value> {
    object_at(ad, "creativeRotation")
        .and_then(|rotation| rotation.get("creativeAssignments"))
        .and_then(Value::as_array)
        .map(|assignments| {
            assignments
                .iter()
                .filter(|ca| truthy(ca.get("creativeId")))
                .map(|ca| (stripped_str(ca, "creativeId"), ca))
                .collect()
        })
        .unwrap_or_default()
}

/// `id` → `enabled` map of an `eventTagOverrides` list.
fn event_tag_override_map(ad: &Value) -> HashMap<String, Value> {
    array_at(ad, "eventTagOverrides")
        .iter()
        .filter(|o| truthy(o.get("id")))
        .map(|o| {
            (
                stripped_str(o, "id"),
                o.get("enabled").cloned().unwrap_or(Value::Null),
            )
        })
        .collect()
}

/// Diffs sheet ad fields against an existing CM360 ad.
///
/// Compares `startTime`, `endTime`, `deliverySchedule`, `dynamicClickTracker`,
/// the ad-level `clickThroughUrl` (`ad_level_clickThroughUrl`),
/// `placementAssignments`, `creativeRotation` (weights are only compared when
/// CM360 has more than one creative in the rotation; creative-level URL
/// changes are reported as `creative_level_clickThroughUrl`) and
/// `eventTagOverrides`. `name` and `type` are never diffed.
pub fn diff_ad(sheet_ad: &Value, cm_ad: &Value) -> (Value, Vec<String>) {
    let mut patch_payload = Map::new();
    let mut diff_fields: Vec<String> = Vec::new();

    // 1. Diff ad dates.
    let iso =
        |ad: &Value, key: &str| normalize_iso_datetime(value_to_opt_string(ad.get(key)).as_deref());
    let sheet_start = iso(sheet_ad, "startTime");
    let cm_start = iso(cm_ad, "startTime");
    if let Some(start) = sheet_start.as_deref().filter(|s| !s.is_empty())
        && sheet_start != cm_start
    {
        patch_payload.insert("startTime".into(), json!(start));
        diff_fields.push("startTime".into());
    }
    let sheet_end = iso(sheet_ad, "endTime");
    let cm_end = iso(cm_ad, "endTime");
    if let Some(end) = sheet_end.as_deref().filter(|s| !s.is_empty())
        && sheet_end != cm_end
    {
        patch_payload.insert("endTime".into(), json!(end));
        diff_fields.push("endTime".into());
    }

    // 2. Diff deliverySchedule (standard / tracking ads).
    if let Some(sheet_ds) = sheet_ad
        .get("deliverySchedule")
        .filter(|ds| truthy(Some(ds)))
    {
        let empty = json!({});
        let cm_ds = cm_ad.get("deliverySchedule").unwrap_or(&empty);
        let s_priority = stripped_str(sheet_ds, "priority");
        let c_priority = stripped_str(cm_ds, "priority");
        let s_ratio = stripped_str(sheet_ds, "impressionRatio");
        let c_ratio = stripped_str(cm_ds, "impressionRatio");
        if (!s_priority.is_empty() && s_priority != c_priority)
            || (!s_ratio.is_empty() && s_ratio != c_ratio)
        {
            patch_payload.insert("deliverySchedule".into(), sheet_ds.clone());
            diff_fields.push("deliverySchedule".into());
        }
    }

    // 3. Diff dynamicClickTracker (click tracker ads).
    if let Some(s_dct_value) = sheet_ad.get("dynamicClickTracker") {
        let s_dct = truthy(Some(s_dct_value));
        let c_dct = truthy(cm_ad.get("dynamicClickTracker"));
        if s_dct != c_dct {
            patch_payload.insert("dynamicClickTracker".into(), json!(s_dct));
            diff_fields.push("dynamicClickTracker".into());
        }
    }

    // 4. Diff ad-level clickThroughUrl (click tracker ads).
    if let Some(sheet_url) = sheet_ad.get("clickThroughUrl") {
        let s_url = custom_click_through_url(sheet_ad);
        let c_url = custom_click_through_url(cm_ad);
        if !s_url.is_empty() && s_url != c_url {
            patch_payload.insert("clickThroughUrl".into(), sheet_url.clone());
            diff_fields.push("ad_level_clickThroughUrl".into());
        }
    }

    // 5. Diff placementAssignments.
    let s_placements = placement_id_set(sheet_ad);
    let c_placements = placement_id_set(cm_ad);
    if !s_placements.is_empty() && s_placements != c_placements {
        patch_payload.insert(
            "placementAssignments".into(),
            sheet_ad
                .get("placementAssignments")
                .cloned()
                .unwrap_or(Value::Null),
        );
        diff_fields.push("placementAssignments".into());
    }

    // 6. Diff creativeRotation (non-click-tracker ads only).
    let s_ad_type = sheet_ad.get("type").and_then(Value::as_str);
    if s_ad_type != Some("AD_SERVING_CLICK_TRACKER") {
        let s_ca_map = creative_assignment_map(sheet_ad);
        if !s_ca_map.is_empty() {
            let c_ca_map = creative_assignment_map(cm_ad);
            let s_keys: HashSet<&String> = s_ca_map.keys().collect();
            let c_keys: HashSet<&String> = c_ca_map.keys().collect();

            let mut rotation_changed = s_keys != c_keys;
            let mut creative_rotation_url_changed = false;
            for cid in s_keys.intersection(&c_keys) {
                let s_ca = s_ca_map[*cid];
                let c_ca = c_ca_map[*cid];
                // When there is a single creative in the rotation CM360 does
                // not assign a weight, so only compare for multi-creative
                // rotations (and only when both weights are present).
                if c_ca_map.len() > 1
                    && let (Some(s_weight), Some(c_weight)) = (
                        value_to_i64(s_ca.get("weight")),
                        value_to_i64(c_ca.get("weight")),
                    )
                    && s_weight != c_weight
                {
                    rotation_changed = true;
                }
                let s_url = custom_click_through_url(s_ca);
                let c_url = custom_click_through_url(c_ca);
                if !s_url.is_empty() && s_url != c_url {
                    creative_rotation_url_changed = true;
                }
            }

            if rotation_changed || creative_rotation_url_changed {
                patch_payload.insert(
                    "creativeRotation".into(),
                    sheet_ad
                        .get("creativeRotation")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                if rotation_changed {
                    diff_fields.push("creativeRotation".into());
                }
                if creative_rotation_url_changed
                    && !diff_fields.iter().any(|f| f == "clickThroughUrl")
                {
                    diff_fields.push("creative_level_clickThroughUrl".into());
                }
            }
        }
    }

    // 7. Diff eventTagOverrides.
    let s_overrides = event_tag_override_map(sheet_ad);
    let c_overrides = event_tag_override_map(cm_ad);
    if s_overrides != c_overrides {
        patch_payload.insert(
            "eventTagOverrides".into(),
            sheet_ad
                .get("eventTagOverrides")
                .cloned()
                .unwrap_or(Value::Null),
        );
        diff_fields.push("eventTagOverrides".into());
    }

    (Value::Object(patch_payload), diff_fields)
}

/// Diffs sheet event tag fields (`type`, `url`, `status`) against an
/// existing CM360 event tag. The `name` is never diffed.
pub fn diff_event_tag(sheet_event_tag: &Value, cm_event_tag: &Value) -> (Value, Vec<String>) {
    let mut patch_payload = Map::new();
    let mut diff_fields: Vec<String> = Vec::new();

    let sheet_type = stripped_str(sheet_event_tag, "type").to_uppercase();
    let cm_type = stripped_str(cm_event_tag, "type").to_uppercase();
    if !sheet_type.is_empty() && sheet_type != cm_type {
        patch_payload.insert("type".into(), json!(sheet_type));
        diff_fields.push("type".into());
    }

    let sheet_url = stripped_str(sheet_event_tag, "url");
    let cm_url = stripped_str(cm_event_tag, "url");
    if !sheet_url.is_empty() && sheet_url != cm_url {
        patch_payload.insert("url".into(), json!(sheet_url));
        diff_fields.push("url".into());
    }

    let sheet_status = stripped_str(sheet_event_tag, "status").to_uppercase();
    let cm_status = stripped_str(cm_event_tag, "status").to_uppercase();
    if !sheet_status.is_empty() && sheet_status != cm_status {
        patch_payload.insert("status".into(), json!(sheet_status));
        diff_fields.push("status".into());
    }

    (Value::Object(patch_payload), diff_fields)
}

/// Extracts the distinct (truthy) placement IDs assigned across all ads, in
/// first-seen order.
pub fn extract_assigned_placement_ids(ads: &EntityMap) -> Vec<String> {
    let mut assigned_ids: Vec<String> = Vec::new();
    for ad_payload in ads.values() {
        for assignment in array_at(ad_payload, "placementAssignments") {
            let p_id = assignment.get("placementId");
            if truthy(p_id) {
                let p_id = value_to_string(p_id);
                if !assigned_ids.contains(&p_id) {
                    assigned_ids.push(p_id);
                }
            }
        }
    }
    assigned_ids
}

/// Merges the existing ad's placement assignments with the sheet payload's
/// assignments: the existing assignments are kept and every expected
/// placement that CM360 does not have yet is appended (once).
pub fn reconcile_missing_ad_placements(
    payload: &mut Value,
    existing_ad: &Value,
    payload_expected_placement_ids: &[String],
    ad_name: &str,
    ad_id: &str,
) {
    let existing_actual_ids: HashSet<String> = placement_id_set(existing_ad);
    let unassigned_ids: Vec<&String> = payload_expected_placement_ids
        .iter()
        .filter(|p| !existing_actual_ids.contains(*p))
        .collect();

    let mut merged: Vec<Value> = array_at(existing_ad, "placementAssignments").to_vec();
    let mut seen_new_ids: HashSet<String> = HashSet::new();
    for assignment in array_at(payload, "placementAssignments") {
        let p_id = stripped_str(assignment, "placementId");
        if unassigned_ids.iter().any(|u| **u == p_id) && !seen_new_ids.contains(&p_id) {
            merged.push(assignment.clone());
            seen_new_ids.insert(p_id);
        }
    }

    if let Some(obj) = payload.as_object_mut() {
        obj.insert("placementAssignments".into(), Value::Array(merged));
    }

    if !unassigned_ids.is_empty() {
        info!(
            "Ad exists but is missing placements. Assigning new placements for {ad_name} (ID: {ad_id})."
        );
    }
}

/// Indexes CM360 resources by their trimmed `name` (only resources with a
/// truthy `name` and `id`; later duplicates overwrite earlier ones).
fn index_by_name(existing: &[Value]) -> ResourceMap {
    let mut by_name = ResourceMap::new();
    for resource in existing {
        if truthy(resource.get("name")) && truthy(resource.get("id")) {
            by_name.insert(
                value_to_string(resource.get("name")).trim().to_string(),
                resource.clone(),
            );
        }
    }
    by_name
}

/// Pure part of `_resolve_placement_ids`: indexes `existing` by trimmed name
/// and rewrites every ad `placementAssignments[].placementId` from a name to
/// the real CM360 ID.
///
/// # Errors
///
/// `"Placement '{name}' not found in Campaign Manager 360."`
pub fn map_placement_ids(ads: &mut EntityMap, existing: &[Value]) -> anyhow::Result<ResourceMap> {
    let existing_by_name = index_by_name(existing);
    for ad_payload in ads.values_mut() {
        let Some(assignments) = ad_payload
            .get_mut("placementAssignments")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for assignment in assignments.iter_mut() {
            let name = value_to_string(assignment.get("placementId"));
            let Some(resource) = existing_by_name.get(&name) else {
                bail!("Placement '{name}' not found in Campaign Manager 360.");
            };
            let real_id = value_to_string(resource.get("id"));
            if let Some(obj) = assignment.as_object_mut() {
                obj.insert("placementId".into(), json!(real_id));
            }
        }
    }
    Ok(existing_by_name)
}

/// Pure part of `_resolve_creative_ids`: indexes `existing` by trimmed name
/// and rewrites every ad `creativeRotation.creativeAssignments[].creativeId`
/// from a name to the real CM360 ID.
///
/// # Errors
///
/// `"Creative '{name}' not found in Campaign Manager 360."`
pub fn map_creative_ids(ads: &mut EntityMap, existing: &[Value]) -> anyhow::Result<ResourceMap> {
    let existing_by_name = index_by_name(existing);
    for ad_payload in ads.values_mut() {
        let Some(assignments) = ad_payload
            .get_mut("creativeRotation")
            .and_then(Value::as_object_mut)
            .and_then(|rotation| rotation.get_mut("creativeAssignments"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for assignment in assignments.iter_mut() {
            let name = value_to_string(assignment.get("creativeId"));
            let Some(resource) = existing_by_name.get(&name) else {
                bail!("Creative '{name}' not found in Campaign Manager 360.");
            };
            let real_id = value_to_string(resource.get("id"));
            if let Some(obj) = assignment.as_object_mut() {
                obj.insert("creativeId".into(), json!(real_id));
            }
        }
    }
    Ok(existing_by_name)
}

/// Fetches the existing CM360 placements and rewrites the ad placement
/// assignments (Python `_resolve_placement_ids`).
///
/// # Errors
///
/// `"Failed to fetch placements from Campaign Manager 360: {e}"` or a
/// [`map_placement_ids`] error.
pub async fn resolve_placement_ids(
    api: &dyn Cm360Api,
    ads: &mut EntityMap,
    profile_id: &str,
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
) -> anyhow::Result<ResourceMap> {
    info!(
        "Fetching existing placements from CM360 for Advertiser ID: {advertiser_id:?}, \
         Campaign ID: {campaign_id:?}"
    );
    let advertiser_ids = advertiser_id
        .filter(|a| !a.is_empty())
        .map(|a| vec![a.to_string()]);
    let campaign_ids = campaign_id
        .filter(|c| !c.is_empty())
        .map(|c| vec![c.to_string()]);
    let existing = match api
        .list_placements(
            profile_id,
            advertiser_ids.as_deref(),
            campaign_ids.as_deref(),
        )
        .await
    {
        Ok(existing) => existing,
        Err(e) => bail!("Failed to fetch placements from Campaign Manager 360: {e}"),
    };
    map_placement_ids(ads, &existing)
}

/// Fetches the existing CM360 creatives and rewrites the ad creative
/// assignments (Python `_resolve_creative_ids`).
///
/// # Errors
///
/// `"Failed to fetch creatives from Campaign Manager 360: {e}"` or a
/// [`map_creative_ids`] error.
pub async fn resolve_creative_ids(
    api: &dyn Cm360Api,
    ads: &mut EntityMap,
    profile_id: &str,
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
) -> anyhow::Result<ResourceMap> {
    info!(
        "Fetching existing creatives from CM360 for Advertiser ID: {advertiser_id:?}, \
         Campaign ID: {campaign_id:?}"
    );
    let existing = match api
        .list_creatives(profile_id, advertiser_id, campaign_id)
        .await
    {
        Ok(existing) => existing,
        Err(e) => bail!("Failed to fetch creatives from Campaign Manager 360: {e}"),
    };
    map_creative_ids(ads, &existing)
}

/// Builds a patch operation record.
fn patch_operation(
    operation: &str,
    id: String,
    name: &str,
    diff_fields: Vec<String>,
    payload: Value,
) -> Operation {
    Operation {
        operation: operation.to_string(),
        id: Some(id),
        name: name.to_string(),
        diff_fields: Some(diff_fields),
        payload,
    }
}

/// Builds an insert operation record.
fn insert_operation(operation: &str, name: &str, payload: Value) -> Operation {
    Operation {
        operation: operation.to_string(),
        id: None,
        name: name.to_string(),
        diff_fields: None,
        payload,
    }
}

/// Pure core of `_resolve_and_build_operations` given the already-fetched
/// existing event tags and ads.
///
/// Produces, in order: placement patches, creative patches, event tag
/// inserts/patches, then ad inserts/patches. Event tag override IDs inside
/// `ads` are rewritten in place to the real CM360 IDs before the ad
/// operations are built from cloned payloads.
#[allow(clippy::too_many_arguments)]
pub fn build_operations(
    ads: &mut EntityMap,
    event_tags: &EntityMap,
    placements: Option<&EntityMap>,
    creatives: Option<&EntityMap>,
    existing_placements: Option<&ResourceMap>,
    existing_creatives: Option<&ResourceMap>,
    existing_event_tags: &[Value],
    existing_ads: &[Value],
) -> Vec<Operation> {
    let existing_event_tags_by_name = index_by_name(existing_event_tags);
    let existing_ads_by_name = index_by_name(existing_ads);

    let mut operations: Vec<Operation> = Vec::new();

    // 1. Placement PATCH operations (diffs only).
    if let (Some(placements), Some(existing_placements)) = (placements, existing_placements)
        && !placements.is_empty()
        && !existing_placements.is_empty()
    {
        for (placement_name, sheet_payload) in placements {
            let clean_name = placement_name.trim();
            if let Some(cm_placement) = existing_placements.get(clean_name) {
                let (patch_payload, diff_fields) = diff_placement(sheet_payload, cm_placement);
                if !diff_fields.is_empty() {
                    operations.push(patch_operation(
                        PLACEMENT_PATCH_OPERATION,
                        value_to_string(cm_placement.get("id")),
                        clean_name,
                        diff_fields,
                        patch_payload,
                    ));
                }
            }
        }
    }

    // 2. Creative PATCH operations (diffs only).
    if let (Some(creatives), Some(existing_creatives)) = (creatives, existing_creatives)
        && !creatives.is_empty()
        && !existing_creatives.is_empty()
    {
        for (creative_name, sheet_payload) in creatives {
            let clean_name = creative_name.trim();
            if let Some(cm_creative) = existing_creatives.get(clean_name) {
                let (patch_payload, diff_fields) = diff_creative(sheet_payload, cm_creative);
                if !diff_fields.is_empty() {
                    operations.push(patch_operation(
                        CREATIVE_PATCH_OPERATION,
                        value_to_string(cm_creative.get("id")),
                        clean_name,
                        diff_fields,
                        patch_payload,
                    ));
                }
            }
        }
    }

    // 3. Event tag operations: INSERT for new tags, PATCH for diffs.
    for (tag_name, raw_payload) in event_tags {
        let mut tag_payload = raw_payload.clone();
        let clean_tag_name = tag_name.trim();
        if let Some(cm_tag) = existing_event_tags_by_name.get(clean_tag_name) {
            let tag_id = value_to_string(cm_tag.get("id"));
            if let Some(obj) = tag_payload.as_object_mut() {
                obj.insert("id".into(), json!(tag_id));
            }
            let (patch_payload, diff_fields) = diff_event_tag(&tag_payload, cm_tag);
            if diff_fields.is_empty() {
                info!(
                    "Event tag '{clean_tag_name}' (ID: {tag_id}) already matches CM360 state. \
                     Skipping patch."
                );
            } else {
                operations.push(patch_operation(
                    EVENT_TAG_PATCH_OPERATION,
                    tag_id,
                    clean_tag_name,
                    diff_fields,
                    patch_payload,
                ));
            }
        } else {
            operations.push(insert_operation(
                EVENT_TAG_INSERT_OPERATION,
                clean_tag_name,
                tag_payload,
            ));
        }
    }

    // 4. Resolve existing event tag IDs in ad overrides (in place).
    for ad_payload in ads.values_mut() {
        let Some(overrides) = ad_payload
            .get_mut("eventTagOverrides")
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for override_entry in overrides.iter_mut() {
            let target_tag = stripped_str(override_entry, "id");
            if let Some(cm_tag) = existing_event_tags_by_name.get(&target_tag)
                && let Some(obj) = override_entry.as_object_mut()
            {
                obj.insert("id".into(), json!(value_to_string(cm_tag.get("id"))));
            }
        }
    }

    // 5. Ad operations: INSERT for new ads, PATCH for diffs.
    for (ad_name, raw_payload) in ads.iter() {
        let mut ad_payload = raw_payload.clone();
        let clean_ad_name = ad_name.trim();
        let payload_expected_placement_ids: Vec<String> =
            array_at(&ad_payload, "placementAssignments")
                .iter()
                .filter(|assignment| truthy(assignment.get("placementId")))
                .map(|assignment| stripped_str(assignment, "placementId"))
                .collect();

        if let Some(existing_ad) = existing_ads_by_name.get(clean_ad_name) {
            let ad_id = value_to_string(existing_ad.get("id"));
            if let Some(obj) = ad_payload.as_object_mut() {
                obj.insert("id".into(), json!(ad_id));
            }
            reconcile_missing_ad_placements(
                &mut ad_payload,
                existing_ad,
                &payload_expected_placement_ids,
                clean_ad_name,
                &ad_id,
            );
            let (patch_payload, diff_fields) = diff_ad(&ad_payload, existing_ad);
            if diff_fields.is_empty() {
                info!(
                    "Ad '{clean_ad_name}' (ID: {ad_id}) already matches CM360 state. Skipping patch."
                );
            } else {
                operations.push(patch_operation(
                    AD_PATCH_OPERATION,
                    ad_id,
                    clean_ad_name,
                    diff_fields,
                    patch_payload,
                ));
            }
        } else {
            operations.push(insert_operation(
                AD_INSERT_OPERATION,
                clean_ad_name,
                ad_payload,
            ));
        }
    }

    operations
}

/// Fetches the existing event tags and ads (ads filtered by the placement IDs
/// assigned in `ads`) and reconciles them into operations via
/// [`build_operations`] (Python `_resolve_and_build_operations`).
#[allow(clippy::too_many_arguments)]
pub async fn resolve_and_build_operations(
    api: &dyn Cm360Api,
    ads: &mut EntityMap,
    event_tags: &EntityMap,
    profile_id: &str,
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
    placements: Option<&EntityMap>,
    creatives: Option<&EntityMap>,
    existing_placements: Option<&ResourceMap>,
    existing_creatives: Option<&ResourceMap>,
) -> anyhow::Result<Vec<Operation>> {
    info!("Fetching existing event tags and ads from CM360 for reconciliation...");
    let assigned_placement_ids = extract_assigned_placement_ids(ads);
    let existing_tags_list = api
        .list_event_tags(profile_id, advertiser_id, campaign_id)
        .await?;
    let campaign_ids = campaign_id
        .filter(|c| !c.is_empty())
        .map(|c| vec![c.to_string()]);
    let existing_ads_list = api
        .list_ads(
            profile_id,
            advertiser_id,
            campaign_ids.as_deref(),
            (!assigned_placement_ids.is_empty()).then_some(assigned_placement_ids.as_slice()),
        )
        .await?;

    Ok(build_operations(
        ads,
        event_tags,
        placements,
        creatives,
        existing_placements,
        existing_creatives,
        &existing_tags_list,
        &existing_ads_list,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::client::test_support::MockApi;
    use super::super::grouping::{group_ads, group_creatives, group_event_tags, group_placements};
    use super::super::sheet::test_support::make_test_csv;
    use super::super::sheet::{parse_csv, prepare_data};
    use super::*;

    fn entity_map(entries: Vec<(&str, Value)>) -> EntityMap {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    #[tokio::test]
    async fn test_resolve_placement_ids() {
        let mut ads = entity_map(vec![(
            "Test Ad",
            json!({
                "name": "Test Ad",
                "placementAssignments": [
                    {"placementId": "Test~Placement~1 Test", "active": true}
                ],
            }),
        )]);
        let api = MockApi::new().with_placements(vec![json!({
            "name": "Test~Placement~1 Test",
            "id": "mock_placement_id_Test~Placement~1 Test",
        })]);
        let existing = resolve_placement_ids(
            &api,
            &mut ads,
            "7023449",
            Some("13641571"),
            Some("30535365"),
        )
        .await
        .unwrap();
        assert_eq!(
            ads["Test Ad"]["placementAssignments"][0]["placementId"],
            "mock_placement_id_Test~Placement~1 Test"
        );
        assert!(existing.contains_key("Test~Placement~1 Test"));
        let calls = api.list_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "list_placements");
        assert_eq!(calls[0].profile_id, "7023449");
        assert_eq!(calls[0].advertiser, vec!["13641571".to_string()]);
        assert_eq!(calls[0].campaigns, vec!["30535365".to_string()]);
    }

    #[tokio::test]
    async fn test_resolve_creative_ids() {
        let mut ads = entity_map(vec![(
            "Test Ad",
            json!({
                "name": "Test Ad",
                "creativeRotation": {
                    "creativeAssignments": [
                        {"creativeId": "TEST_ACG~300x250", "active": true}
                    ]
                },
            }),
        )]);
        let api = MockApi::new()
            .with_creatives(vec![json!({"name": "TEST_ACG~300x250", "id": "999123"})]);
        resolve_creative_ids(
            &api,
            &mut ads,
            "7023449",
            Some("13641571"),
            Some("30535365"),
        )
        .await
        .unwrap();
        assert_eq!(
            ads["Test Ad"]["creativeRotation"]["creativeAssignments"][0]["creativeId"],
            "999123"
        );
        let calls = api.list_calls();
        assert_eq!(calls[0].method, "list_creatives");
        assert_eq!(calls[0].advertiser, vec!["13641571".to_string()]);
        assert_eq!(calls[0].campaigns, vec!["30535365".to_string()]);
    }

    #[tokio::test]
    async fn resolve_ids_error_paths() {
        let mut ads = entity_map(vec![(
            "Ad",
            json!({
                "placementAssignments": [{"placementId": "Missing"}],
                "creativeRotation": {"creativeAssignments": [{"creativeId": "Nope"}]},
            }),
        )]);
        let api = MockApi::new();
        let err = resolve_placement_ids(&api, &mut ads, "1", None, None)
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Placement 'Missing' not found in Campaign Manager 360."
        );
        let err = resolve_creative_ids(&api, &mut ads, "1", None, None)
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Creative 'Nope' not found in Campaign Manager 360."
        );
        // Without filters the list calls receive no IDs.
        let calls = api.list_calls();
        assert!(calls[0].advertiser.is_empty() && calls[0].campaigns.is_empty());

        let failing = MockApi::new().with_list_error("boom");
        let err = resolve_placement_ids(&failing, &mut ads, "1", Some("a"), Some("c"))
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Failed to fetch placements from Campaign Manager 360: boom"
        );
        let err = resolve_creative_ids(&failing, &mut ads, "1", None, None)
            .await
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Failed to fetch creatives from Campaign Manager 360: boom"
        );
    }

    #[test]
    fn map_ids_handles_numeric_ids_and_whitespace_names() {
        let mut ads = entity_map(vec![(
            "Ad",
            json!({
                "placementAssignments": [{"placementId": "P1"}, {"placementId": "P1"}],
                "creativeRotation": {"creativeAssignments": [{"creativeId": "C1"}]},
            }),
        )]);
        let placements = vec![
            json!({"name": " P1 ", "id": 42}),
            json!({"name": "NoId"}),
            json!({"id": "no-name"}),
        ];
        let existing = map_placement_ids(&mut ads, &placements).unwrap();
        assert_eq!(existing.keys().collect::<Vec<_>>(), vec!["P1"]);
        assert_eq!(
            ads["Ad"]["placementAssignments"],
            json!([{"placementId": "42"}, {"placementId": "42"}])
        );
        let creatives = vec![json!({"name": "C1", "id": "7"})];
        map_creative_ids(&mut ads, &creatives).unwrap();
        assert_eq!(
            ads["Ad"]["creativeRotation"]["creativeAssignments"][0]["creativeId"],
            "7"
        );
        // Ads without rotation / assignments are skipped.
        let mut click_tracker = entity_map(vec![("CT", json!({"name": "CT"}))]);
        assert!(
            map_creative_ids(&mut click_tracker, &creatives)
                .unwrap()
                .len()
                == 1
        );
        assert!(map_placement_ids(&mut click_tracker, &placements).is_ok());
    }

    #[tokio::test]
    async fn test_resolve_and_build_operations_with_existing_entities() {
        let mut ads = entity_map(vec![
            (
                "Existing Ad",
                json!({
                    "name": "Existing Ad",
                    "type": "AD_SERVING_STANDARD_AD",
                    "placementAssignments": [{"placementId": "mock_placement_2"}],
                    "eventTagOverrides": [{"id": "Existing Tag"}, {"id": "New Tag"}],
                }),
            ),
            (
                "New Ad",
                json!({
                    "name": "New Ad",
                    "type": "AD_SERVING_STANDARD_AD",
                    "placementAssignments": [{"placementId": "mock_placement_1"}],
                    "eventTagOverrides": [],
                }),
            ),
        ]);
        let event_tags = entity_map(vec![
            (
                "Existing Tag",
                json!({"name": "Existing Tag", "url": "https://example.com/tag"}),
            ),
            (
                "New Tag",
                json!({"name": "New Tag", "url": "https://example.com/new_tag"}),
            ),
        ]);
        let api = MockApi::new()
            .with_event_tags(vec![json!({"name": "Existing Tag", "id": "tag_999"})])
            .with_ads(vec![json!({
                "name": "Existing Ad",
                "id": "ad_888",
                "placementAssignments": [{"placementId": "mock_placement_1"}],
            })]);

        let ops = resolve_and_build_operations(
            &api,
            &mut ads,
            &event_tags,
            "7023449",
            Some("13641571"),
            Some("30535365"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(ops.len(), 4);
        let existing_tag_op = ops.iter().find(|op| op.name == "Existing Tag").unwrap();
        assert_eq!(existing_tag_op.operation, "dfareporting.eventTags.patch");
        assert_eq!(existing_tag_op.id.as_deref(), Some("tag_999"));
        assert!(
            existing_tag_op
                .diff_fields
                .as_ref()
                .unwrap()
                .contains(&"url".to_string())
        );
        assert_eq!(existing_tag_op.payload["url"], "https://example.com/tag");

        let new_tag_op = ops.iter().find(|op| op.name == "New Tag").unwrap();
        assert_eq!(new_tag_op.operation, "dfareporting.eventTags.insert");
        assert!(new_tag_op.payload.get("id").is_none());
        assert!(new_tag_op.id.is_none());
        assert!(new_tag_op.diff_fields.is_none());

        let existing_ad_op = ops.iter().find(|op| op.name == "Existing Ad").unwrap();
        assert_eq!(existing_ad_op.operation, "dfareporting.ads.patch");
        assert_eq!(existing_ad_op.id.as_deref(), Some("ad_888"));
        let placement_ids: Vec<&str> = existing_ad_op.payload["placementAssignments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["placementId"].as_str().unwrap())
            .collect();
        assert_eq!(placement_ids, vec!["mock_placement_1", "mock_placement_2"]);
        let overrides = existing_ad_op.payload["eventTagOverrides"]
            .as_array()
            .unwrap();
        assert_eq!(overrides[0]["id"], "tag_999");
        assert_eq!(overrides[1]["id"], "New Tag");
        assert_eq!(
            existing_ad_op.diff_fields.as_ref().unwrap(),
            &vec![
                "placementAssignments".to_string(),
                "eventTagOverrides".to_string()
            ]
        );
        // The patch payload only contains the changed fields.
        assert!(existing_ad_op.payload.get("name").is_none());

        let new_ad_op = ops.iter().find(|op| op.name == "New Ad").unwrap();
        assert_eq!(new_ad_op.operation, "dfareporting.ads.insert");
        assert!(new_ad_op.payload.get("id").is_none());

        // Ordering: event tags first, then ads (grouping order).
        assert_eq!(
            ops.iter().map(|op| op.name.as_str()).collect::<Vec<_>>(),
            vec!["Existing Tag", "New Tag", "Existing Ad", "New Ad"]
        );

        // Override IDs were rewritten in place in `ads`; the original ad map
        // did not receive the `id` key.
        assert_eq!(ads["Existing Ad"]["eventTagOverrides"][0]["id"], "tag_999");
        assert!(ads["Existing Ad"].get("id").is_none());

        // The ads list call was filtered by the assigned placement IDs.
        let calls = api.list_calls();
        assert_eq!(calls[0].method, "list_event_tags");
        assert_eq!(calls[0].advertiser, vec!["13641571".to_string()]);
        assert_eq!(calls[0].campaigns, vec!["30535365".to_string()]);
        assert_eq!(calls[1].method, "list_ads");
        assert_eq!(calls[1].campaigns, vec!["30535365".to_string()]);
        assert_eq!(
            calls[1].placements,
            vec![
                "mock_placement_2".to_string(),
                "mock_placement_1".to_string()
            ]
        );
    }

    #[test]
    fn test_extract_assigned_placement_ids() {
        let ads = entity_map(vec![
            (
                "Ad 1",
                json!({"placementAssignments": [{"placementId": "p1"}, {"placementId": "p2"}]}),
            ),
            (
                "Ad 2",
                json!({"placementAssignments": [{"placementId": "p2"}, {"placementId": "p3"}, {"placementId": ""}, {"placementId": 4}]}),
            ),
            ("Ad 3", json!({"name": "no assignments"})),
        ]);
        assert_eq!(
            extract_assigned_placement_ids(&ads),
            vec!["p1", "p2", "p3", "4"]
        );
    }

    #[test]
    fn test_diff_placement_never_diffs_name() {
        let sheet_placement = json!({
            "name": "New Placement Name In Sheet",
            "activeStatus": "PLACEMENT_STATUS_ACTIVE",
            "size": {"width": 300, "height": 250},
            "pricingSchedule": {"startDate": "2026-06-01", "endDate": "2026-06-30"},
        });
        let cm_placement = json!({
            "name": "Old Placement Name In CM360",
            "activeStatus": "PLACEMENT_STATUS_ACTIVE",
            "size": {"width": 300, "height": 250},
            "pricingSchedule": {"startDate": "2026-06-01", "endDate": "2026-06-30"},
        });
        let (patch_payload, diff_fields) = diff_placement(&sheet_placement, &cm_placement);
        assert_eq!(diff_fields, Vec::<String>::new());
        assert_eq!(patch_payload, json!({}));
    }

    #[test]
    fn test_diff_placement_diffs_active_status_and_size() {
        let sheet_placement = json!({
            "name": "Placement Inactive in Sheet",
            "activeStatus": "PLACEMENT_STATUS_INACTIVE",
            "size": {"width": 728, "height": 90},
        });
        let cm_placement = json!({
            "name": "Placement Inactive in Sheet",
            "activeStatus": "PLACEMENT_STATUS_ACTIVE",
            "size": {"width": "300", "height": "250"},
        });
        let (patch_payload, diff_fields) = diff_placement(&sheet_placement, &cm_placement);
        assert_eq!(diff_fields, vec!["activeStatus", "size"]);
        assert_eq!(
            patch_payload,
            json!({"activeStatus": "PLACEMENT_STATUS_INACTIVE", "size": {"width": 728, "height": 90}})
        );
        // Unknown sheet size / empty status never produce a diff.
        let (patch_payload, diff_fields) = diff_placement(
            &json!({"activeStatus": "", "size": {"width": 0, "height": 0}}),
            &cm_placement,
        );
        assert!(diff_fields.is_empty());
        assert_eq!(patch_payload, json!({}));
    }

    #[test]
    fn test_diff_placement_aligns_pricing_periods() {
        let sheet_placement = json!({
            "name": "Placement A",
            "pricingSchedule": {"startDate": "2026-09-07", "endDate": "2026-09-30"},
        });
        let cm_placement = json!({
            "name": "Placement A",
            "pricingSchedule": {
                "startDate": "2026-06-10",
                "endDate": "2026-07-10",
                "pricingPeriods": [{
                    "startDate": "2026-06-10",
                    "endDate": "2026-07-10",
                    "units": "1000",
                    "rateOrCostNanos": "5000000000",
                }],
            },
        });
        let (patch_payload, diff_fields) = diff_placement(&sheet_placement, &cm_placement);
        assert!(diff_fields.contains(&"pricingSchedule.startDate".to_string()));
        assert!(diff_fields.contains(&"pricingSchedule.endDate".to_string()));
        assert_eq!(patch_payload["pricingSchedule"]["startDate"], "2026-09-07");
        assert_eq!(patch_payload["pricingSchedule"]["endDate"], "2026-09-30");
        assert_eq!(
            patch_payload["pricingSchedule"]["pricingPeriods"],
            json!([{
                "startDate": "2026-09-07",
                "endDate": "2026-09-30",
                "units": "1000",
                "rateOrCostNanos": "5000000000",
            }])
        );
        // Key order of the patched period is preserved.
        assert_eq!(
            patch_payload["pricingSchedule"]["pricingPeriods"][0].to_string(),
            r#"{"startDate":"2026-09-07","endDate":"2026-09-30","units":"1000","rateOrCostNanos":"5000000000"}"#
        );
    }

    #[test]
    fn diff_placement_creates_pricing_period_when_none_exist() {
        let sheet_placement = json!({
            "pricingSchedule": {"startDate": "2026-09-07T00:00:00Z", "endDate": null},
        });
        let cm_placement = json!({
            "pricingSchedule": {"startDate": "2026-06-10", "endDate": "2026-07-10"},
        });
        let (patch_payload, diff_fields) = diff_placement(&sheet_placement, &cm_placement);
        assert_eq!(diff_fields, vec!["pricingSchedule.startDate"]);
        assert_eq!(
            patch_payload,
            json!({
                "pricingSchedule": {
                    "startDate": "2026-09-07",
                    "pricingPeriods": [{"startDate": "2026-09-07", "endDate": "2026-07-10"}],
                }
            })
        );
    }

    #[test]
    fn test_diff_creative_never_diffs_name() {
        let (patch_payload, diff_fields) = diff_creative(
            &json!({"name": "Creative Renamed in Sheet", "size": {"width": 300, "height": 250}}),
            &json!({"name": "Original Creative Name", "size": {"width": 300, "height": 250}}),
        );
        assert!(diff_fields.is_empty());
        assert_eq!(patch_payload, json!({}));

        let (patch_payload, diff_fields) = diff_creative(
            &json!({"size": {"width": 300, "height": 600}}),
            &json!({"size": {"width": 300, "height": 250}}),
        );
        assert_eq!(diff_fields, vec!["size"]);
        assert_eq!(
            patch_payload,
            json!({"size": {"width": 300, "height": 600}})
        );
    }

    #[test]
    fn test_diff_ad_never_diffs_name() {
        let sheet_ad = json!({
            "name": "Sheet Renamed Ad",
            "startTime": "2026-06-01T00:00:00Z",
            "endTime": "2026-06-30T00:00:00Z",
            "placementAssignments": [{"placementId": "p1"}],
            "eventTagOverrides": [],
        });
        let cm_ad = json!({
            "name": "CM360 Ad Name",
            "startTime": "2026-06-01T00:00:00Z",
            "endTime": "2026-06-30T00:00:00Z",
            "placementAssignments": [{"placementId": "p1"}],
            "eventTagOverrides": [],
        });
        let (patch_payload, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert!(diff_fields.is_empty());
        assert_eq!(patch_payload, json!({}));
    }

    #[test]
    fn diff_ad_dates_schedule_tracker_and_url() {
        let sheet_ad = json!({
            "type": "AD_SERVING_CLICK_TRACKER",
            "startTime": "2026-06-02T00:00:00Z",
            "endTime": "2026-06-30",
            "deliverySchedule": {"priority": "AD_PRIORITY_02", "impressionRatio": "1"},
            "dynamicClickTracker": true,
            "clickThroughUrl": {"customClickThroughUrl": "https://new.example"},
            "placementAssignments": [{"placementId": "p1"}],
            "eventTagOverrides": [],
        });
        let cm_ad = json!({
            "startTime": "2026-06-01T00:00:00.000Z",
            "endTime": "2026-06-30T00:00:00Z",
            "deliverySchedule": {"priority": "AD_PRIORITY_01", "impressionRatio": "1"},
            "dynamicClickTracker": false,
            "clickThroughUrl": {"customClickThroughUrl": "https://old.example"},
            "placementAssignments": [{"placementId": "p1"}],
        });
        let (patch_payload, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(
            diff_fields,
            vec![
                "startTime",
                "deliverySchedule",
                "dynamicClickTracker",
                "ad_level_clickThroughUrl"
            ]
        );
        assert_eq!(
            patch_payload,
            json!({
                "startTime": "2026-06-02T00:00:00Z",
                "deliverySchedule": {"priority": "AD_PRIORITY_02", "impressionRatio": "1"},
                "dynamicClickTracker": true,
                "clickThroughUrl": {"customClickThroughUrl": "https://new.example"},
            })
        );
        // Click trackers never diff creative rotation.
        let sheet_ad = json!({
            "type": "AD_SERVING_CLICK_TRACKER",
            "creativeRotation": {"creativeAssignments": [{"creativeId": "c1", "weight": 1}]},
        });
        let (_, diff_fields) = diff_ad(&sheet_ad, &json!({}));
        assert!(diff_fields.is_empty());
    }

    #[test]
    fn diff_ad_creative_rotation_rules() {
        // Different creative sets → creativeRotation.
        let sheet_ad = json!({
            "type": "AD_SERVING_STANDARD_AD",
            "creativeRotation": {"creativeAssignments": [{"creativeId": "c1", "weight": 50}]},
        });
        let cm_ad = json!({
            "creativeRotation": {"creativeAssignments": [{"creativeId": "c2", "weight": 50}]},
        });
        let (patch_payload, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(diff_fields, vec!["creativeRotation"]);
        assert_eq!(
            patch_payload["creativeRotation"],
            sheet_ad["creativeRotation"]
        );

        // Single creative in CM: weight differences are ignored.
        let sheet_ad = json!({
            "type": "AD_SERVING_STANDARD_AD",
            "creativeRotation": {"creativeAssignments": [{"creativeId": "c1", "weight": 100}]},
        });
        let cm_ad = json!({
            "creativeRotation": {"creativeAssignments": [{"creativeId": "c1"}]},
        });
        let (_, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert!(diff_fields.is_empty());

        // Multiple creatives in CM: weight differences count.
        let sheet_ad = json!({
            "type": "AD_SERVING_STANDARD_AD",
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 70},
                {"creativeId": "c2", "weight": 30},
            ]},
        });
        let cm_ad = json!({
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 50},
                {"creativeId": "c2", "weight": 50},
            ]},
        });
        let (_, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(diff_fields, vec!["creativeRotation"]);

        // Creative-level click-through URL change.
        let sheet_ad = json!({
            "type": "AD_SERVING_STANDARD_AD",
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 1, "clickThroughUrl": {"customClickThroughUrl": "https://new"}},
            ]},
        });
        let cm_ad = json!({
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 1, "clickThroughUrl": {"customClickThroughUrl": "https://old"}},
            ]},
        });
        let (patch_payload, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(diff_fields, vec!["creative_level_clickThroughUrl"]);
        assert_eq!(
            patch_payload["creativeRotation"],
            sheet_ad["creativeRotation"]
        );

        // Both weight and URL changes.
        let sheet_ad = json!({
            "type": "AD_SERVING_STANDARD_AD",
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 70, "clickThroughUrl": {"customClickThroughUrl": "https://new"}},
                {"creativeId": "c2", "weight": 30},
            ]},
        });
        let cm_ad = json!({
            "creativeRotation": {"creativeAssignments": [
                {"creativeId": "c1", "weight": 50},
                {"creativeId": "c2", "weight": 50},
            ]},
        });
        let (_, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(
            diff_fields,
            vec!["creativeRotation", "creative_level_clickThroughUrl"]
        );

        // No sheet assignments → nothing to compare.
        let (_, diff_fields) = diff_ad(
            &json!({"creativeRotation": {"creativeAssignments": []}}),
            &cm_ad,
        );
        assert!(diff_fields.is_empty());
    }

    #[test]
    fn diff_ad_placements_and_event_tag_overrides() {
        let sheet_ad = json!({
            "placementAssignments": [{"placementId": " p1 "}, {"placementId": "p2"}],
            "eventTagOverrides": [{"id": "t1", "enabled": true}],
        });
        let cm_ad = json!({
            "placementAssignments": [{"placementId": "p1"}],
            "eventTagOverrides": [{"id": "t1", "enabled": false}],
        });
        let (patch_payload, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert_eq!(
            diff_fields,
            vec!["placementAssignments", "eventTagOverrides"]
        );
        assert_eq!(
            patch_payload["placementAssignments"],
            sheet_ad["placementAssignments"]
        );
        assert_eq!(
            patch_payload["eventTagOverrides"],
            sheet_ad["eventTagOverrides"]
        );

        // Same sets (order independent) → no diff; numeric IDs compare as strings.
        let sheet_ad = json!({
            "placementAssignments": [{"placementId": "p2"}, {"placementId": "p1"}],
            "eventTagOverrides": [{"id": 5, "enabled": true}],
        });
        let cm_ad = json!({
            "placementAssignments": [{"placementId": "p1"}, {"placementId": "p2"}],
            "eventTagOverrides": [{"id": "5", "enabled": true}],
        });
        let (_, diff_fields) = diff_ad(&sheet_ad, &cm_ad);
        assert!(diff_fields.is_empty());

        // Empty sheet placements never diff, but an empty sheet override list
        // against existing CM360 overrides does (the map comparison is strict).
        let (patch_payload, diff_fields) = diff_ad(&json!({}), &cm_ad);
        assert_eq!(diff_fields, vec!["eventTagOverrides"]);
        assert_eq!(patch_payload["eventTagOverrides"], Value::Null);
        // Missing overrides on both sides are equal.
        let (_, diff_fields) = diff_ad(
            &json!({}),
            &json!({"placementAssignments": [{"placementId": "p1"}]}),
        );
        assert!(diff_fields.is_empty());
    }

    #[test]
    fn test_diff_event_tag_never_diffs_name() {
        let sheet_tag = json!({
            "name": "Sheet Event Tag Name",
            "type": "IMPRESSION_IMAGE_EVENT_TAG",
            "url": "https://example.com/tag",
            "status": "ENABLED",
        });
        let cm_tag_identical = json!({
            "name": "Different Name In CM360",
            "type": "IMPRESSION_IMAGE_EVENT_TAG",
            "url": "https://example.com/tag",
            "status": "ENABLED",
        });
        let (patch_payload, diff_fields) = diff_event_tag(&sheet_tag, &cm_tag_identical);
        assert!(diff_fields.is_empty());
        assert_eq!(patch_payload, json!({}));

        let cm_tag_diff = json!({
            "name": "Different Name In CM360",
            "type": "CLICK_THROUGH_EVENT_TAG",
            "url": "https://example.com/old_url",
            "status": "DISABLED",
        });
        let (patch_payload, diff_fields) = diff_event_tag(&sheet_tag, &cm_tag_diff);
        assert_eq!(diff_fields, vec!["type", "url", "status"]);
        assert_eq!(
            patch_payload,
            json!({
                "type": "IMPRESSION_IMAGE_EVENT_TAG",
                "url": "https://example.com/tag",
                "status": "ENABLED",
            })
        );
        // Case-insensitive type/status comparison; empty sheet values ignored.
        let (_, diff_fields) = diff_event_tag(
            &json!({"type": "impression_image_event_tag", "url": "", "status": "enabled"}),
            &cm_tag_identical,
        );
        assert!(diff_fields.is_empty());
    }

    #[test]
    fn test_reconcile_missing_ad_placements() {
        let mut payload = json!({
            "placementAssignments": [
                {"placementId": "p1", "active": true},
                {"placementId": "p2", "active": true},
                {"placementId": "p2", "active": true},
                {"placementId": "p3", "active": true},
            ]
        });
        let existing_ad = json!({
            "placementAssignments": [{"placementId": "p1", "active": false, "sslRequired": true}]
        });
        reconcile_missing_ad_placements(
            &mut payload,
            &existing_ad,
            &["p1".to_string(), "p2".to_string()],
            "Ad",
            "ad_1",
        );
        assert_eq!(
            payload["placementAssignments"],
            json!([
                {"placementId": "p1", "active": false, "sslRequired": true},
                {"placementId": "p2", "active": true},
            ])
        );
        // Nothing missing: the existing assignments replace the payload's.
        let mut payload = json!({"placementAssignments": [{"placementId": "p1"}]});
        reconcile_missing_ad_placements(&mut payload, &existing_ad, &["p1".to_string()], "Ad", "1");
        assert_eq!(
            payload["placementAssignments"],
            existing_ad["placementAssignments"]
        );
    }

    #[tokio::test]
    async fn test_resolve_and_build_operations_skips_identical_event_tags() {
        let event_tags = entity_map(vec![(
            "Sync Tag",
            json!({
                "name": "Sync Tag",
                "type": "IMPRESSION_IMAGE_EVENT_TAG",
                "url": "https://example.com/tag",
                "status": "ENABLED",
            }),
        )]);
        let api = MockApi::new().with_event_tags(vec![json!({
            "name": "Sync Tag",
            "id": "tag_123",
            "type": "IMPRESSION_IMAGE_EVENT_TAG",
            "url": "https://example.com/tag",
            "status": "ENABLED",
        })]);
        let mut ads = EntityMap::new();
        let ops = resolve_and_build_operations(
            &api,
            &mut ads,
            &event_tags,
            "7023449",
            Some("13641571"),
            Some("30535365"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert!(ops.is_empty());
        // No assigned placements → the ads list call gets no placement filter.
        let calls = api.list_calls();
        assert_eq!(calls[1].method, "list_ads");
        assert!(calls[1].placements.is_empty());
    }

    #[tokio::test]
    async fn resolve_and_build_operations_propagates_list_errors() {
        let api = MockApi::new().with_list_error("down");
        let mut ads = EntityMap::new();
        let err = resolve_and_build_operations(
            &api,
            &mut ads,
            &EntityMap::new(),
            "1",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(err.to_string(), "down");
    }

    #[test]
    fn build_operations_emits_placement_and_creative_patches_first() {
        let placements = entity_map(vec![
            (
                "P1 ",
                json!({"name": "P1", "activeStatus": "PLACEMENT_STATUS_INACTIVE", "size": {"width": 0, "height": 0}}),
            ),
            (
                "P2",
                json!({"name": "P2", "activeStatus": "PLACEMENT_STATUS_ACTIVE"}),
            ),
            (
                "P3",
                json!({"name": "P3", "activeStatus": "PLACEMENT_STATUS_ACTIVE"}),
            ),
        ]);
        let creatives = entity_map(vec![
            (
                "C1",
                json!({"name": "C1", "size": {"width": 300, "height": 600}}),
            ),
            (
                "C2",
                json!({"name": "C2", "size": {"width": 300, "height": 250}}),
            ),
        ]);
        let existing_placements: ResourceMap = [
            (
                "P1".to_string(),
                json!({"id": 11, "name": "P1", "activeStatus": "PLACEMENT_STATUS_ACTIVE"}),
            ),
            (
                "P2".to_string(),
                json!({"id": "12", "name": "P2", "activeStatus": "PLACEMENT_STATUS_ACTIVE"}),
            ),
        ]
        .into_iter()
        .collect();
        let existing_creatives: ResourceMap = [
            (
                "C1".to_string(),
                json!({"id": "21", "name": "C1", "size": {"width": 300, "height": 250}}),
            ),
            (
                "C2".to_string(),
                json!({"id": "22", "name": "C2", "size": {"width": 300, "height": 250}}),
            ),
        ]
        .into_iter()
        .collect();
        let mut ads = entity_map(vec![(
            "Ad",
            json!({"name": "Ad", "eventTagOverrides": [{"id": "T"}]}),
        )]);
        let event_tags = entity_map(vec![("T", json!({"name": "T", "url": "https://t"}))]);

        let ops = build_operations(
            &mut ads,
            &event_tags,
            Some(&placements),
            Some(&creatives),
            Some(&existing_placements),
            Some(&existing_creatives),
            &[json!({"name": "T", "id": "t1", "url": "https://t"})],
            &[],
        );
        assert_eq!(
            ops.iter()
                .map(|op| (op.operation.as_str(), op.name.as_str(), op.id.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                ("dfareporting.placements.patch", "P1", Some("11")),
                ("dfareporting.creatives.patch", "C1", Some("21")),
                ("dfareporting.ads.insert", "Ad", None),
            ]
        );
        assert_eq!(
            ops[0].payload,
            json!({"activeStatus": "PLACEMENT_STATUS_INACTIVE"})
        );
        assert_eq!(
            ops[0].diff_fields.as_deref(),
            Some(&["activeStatus".to_string()][..])
        );
        assert_eq!(
            ops[1].payload,
            json!({"size": {"width": 300, "height": 600}})
        );
        // The identical event tag was skipped but its ID was resolved in the ad.
        assert_eq!(ops[2].payload["eventTagOverrides"][0]["id"], "t1");
        assert_eq!(
            serde_json::to_value(&ops[0]).unwrap(),
            json!({
                "operation": "dfareporting.placements.patch",
                "id": "11",
                "name": "P1",
                "diff_fields": ["activeStatus"],
                "payload": {"activeStatus": "PLACEMENT_STATUS_INACTIVE"},
            })
        );

        // Empty maps disable the placement / creative passes entirely.
        let ops = build_operations(
            &mut ads,
            &EntityMap::new(),
            Some(&placements),
            Some(&creatives),
            Some(&ResourceMap::new()),
            None,
            &[],
            &[],
        );
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].operation, "dfareporting.ads.insert");
    }

    #[test]
    fn operation_deserialises_with_optional_fields() {
        let op: Operation =
            serde_json::from_str(r#"{"operation": "dfareporting.ads.insert", "payload": {}}"#)
                .unwrap();
        assert_eq!(op.name, "");
        assert!(op.id.is_none() && op.diff_fields.is_none());
        let op: Operation = serde_json::from_value(json!({
            "operation": "dfareporting.ads.patch",
            "id": "1",
            "name": "A",
            "diff_fields": ["startTime"],
            "payload": {"startTime": "x"},
        }))
        .unwrap();
        assert_eq!(op.diff_fields, Some(vec!["startTime".to_string()]));
    }

    /// End-to-end port of `test_parse_trafficking_sheet_success`: parse →
    /// group → resolve IDs → build operations.
    #[tokio::test]
    async fn full_pipeline_from_test_csv() {
        let raw = parse_csv(&make_test_csv(&[])).unwrap();
        let (rows, meta) = prepare_data(&raw).unwrap();
        let placements = group_placements(
            &rows,
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
            meta.campaign_name.as_deref(),
        );
        let creatives = group_creatives(&rows, Some(&meta.advertiser_id));
        let event_tags =
            group_event_tags(&rows, Some(&meta.advertiser_id), Some(&meta.campaign_id));
        let mut ads = group_ads(
            &rows,
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
            meta.campaign_name.as_deref(),
        );

        let placement_names = [
            "Test~Placement~1 Test",
            "Test~Placement~2 Test",
            "Summer Placement",
            "Test~Placement~1",
        ];
        let api = MockApi::new()
            .with_placements(
                placement_names
                    .iter()
                    .map(|n| json!({"name": n, "id": format!("mock_placement_id_{n}")}))
                    .collect(),
            )
            .with_creatives(vec![
                json!({"name": "TEST_ACG~300x250", "id": "999123"}),
                json!({"name": "TEST_Shopathon_300x250", "id": "999456"}),
            ]);

        let existing_placements = resolve_placement_ids(
            &api,
            &mut ads,
            &meta.profile_id,
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
        )
        .await
        .unwrap();
        let existing_creatives = resolve_creative_ids(
            &api,
            &mut ads,
            &meta.profile_id,
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
        )
        .await
        .unwrap();
        let ops = resolve_and_build_operations(
            &api,
            &mut ads,
            &event_tags,
            &meta.profile_id,
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
            Some(&placements),
            Some(&creatives),
            Some(&existing_placements),
            Some(&existing_creatives),
        )
        .await
        .unwrap();

        let op_types: Vec<&str> = ops.iter().map(|op| op.operation.as_str()).collect();
        assert!(!op_types.contains(&"dfareporting.creatives.insert"));
        assert!(op_types.contains(&"dfareporting.ads.insert"));
        // The mocked CM360 placement / creative carry no size, so the 300x250
        // sheet entities produce size patches ahead of the ad insert (exactly
        // what the Python pipeline yields with the same mocks).
        assert_eq!(
            op_types,
            vec![
                "dfareporting.placements.patch",
                "dfareporting.creatives.patch",
                "dfareporting.ads.insert",
            ]
        );
        assert_eq!(ops[0].name, "Test~Placement~1 Test");
        assert_eq!(
            ops[0].id.as_deref(),
            Some("mock_placement_id_Test~Placement~1 Test")
        );
        assert_eq!(
            ops[0].payload,
            json!({"activeStatus": "PLACEMENT_STATUS_ACTIVE", "size": {"width": 300, "height": 250}})
        );
        assert_eq!(
            ops[0].diff_fields.as_deref(),
            Some(&["activeStatus".to_string(), "size".to_string()][..])
        );
        assert_eq!(ops[1].name, "TEST_ACG~300x250");
        assert_eq!(ops[1].id.as_deref(), Some("999123"));
        assert_eq!(
            ops[1].diff_fields.as_deref(),
            Some(&["size".to_string()][..])
        );

        let ad_c = &ops[2].payload;
        assert_eq!(ad_c["name"], "Test Ad C");
        let assignments = ad_c["creativeRotation"]["creativeAssignments"]
            .as_array()
            .unwrap();
        assert_eq!(assignments.len(), 1);
        assert_eq!(assignments[0]["creativeId"], "999123");
        assert_eq!(
            assignments[0]["clickThroughUrl"]["customClickThroughUrl"],
            "https://www.test.com?utm_medium=test"
        );
        let schedule = &ad_c["deliverySchedule"];
        assert_eq!(schedule["priority"], "AD_PRIORITY_01");
        assert_eq!(schedule["impressionRatio"], "1");
        let placement_assignments = ad_c["placementAssignments"].as_array().unwrap();
        assert_eq!(placement_assignments.len(), 1);
        assert_eq!(placement_assignments[0]["active"], true);
        assert_eq!(
            placement_assignments[0]["placementId"],
            "mock_placement_id_Test~Placement~1 Test"
        );
        assert_eq!(existing_placements.len(), 4);
        assert_eq!(existing_creatives.len(), 2);
    }

    #[tokio::test]
    async fn full_pipeline_emits_placement_patch_when_cm_differs() {
        let raw = parse_csv(&make_test_csv(&[(
            "placement_status",
            "PLACEMENT_STATUS_INACTIVE",
        )]))
        .unwrap();
        let (rows, meta) = prepare_data(&raw).unwrap();
        let placements = group_placements(&rows, None, Some(&meta.campaign_id), None);
        let mut ads = group_ads(&rows, None, Some(&meta.campaign_id), None);
        let api = MockApi::new()
            .with_placements(vec![json!({
                "name": "Test~Placement~1 Test",
                "id": "p1",
                "activeStatus": "PLACEMENT_STATUS_ACTIVE",
                "size": {"width": 300, "height": 250},
            })])
            .with_creatives(vec![json!({"name": "TEST_ACG~300x250", "id": "999123"})])
            .with_ads(vec![json!({
                "name": "Test Ad C",
                "id": "ad1",
                "startTime": "3000-08-29T00:00:00Z",
                "endTime": "3000-09-25T00:00:00Z",
                "deliverySchedule": {"priority": "AD_PRIORITY_01", "impressionRatio": "1"},
                "placementAssignments": [{"placementId": "p1", "active": true}],
                "creativeRotation": {"creativeAssignments": [{
                    "creativeId": "999123",
                    "clickThroughUrl": {"customClickThroughUrl": "https://www.test.com?utm_medium=test"},
                }]},
                "eventTagOverrides": [],
            })]);
        let existing_placements = resolve_placement_ids(&api, &mut ads, "1", None, None)
            .await
            .unwrap();
        let existing_creatives = resolve_creative_ids(&api, &mut ads, "1", None, None)
            .await
            .unwrap();
        let ops = resolve_and_build_operations(
            &api,
            &mut ads,
            &EntityMap::new(),
            "1",
            None,
            None,
            Some(&placements),
            None,
            Some(&existing_placements),
            Some(&existing_creatives),
        )
        .await
        .unwrap();
        // Only the placement status differs; the ad already matches CM360.
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].operation, "dfareporting.placements.patch");
        assert_eq!(ops[0].id.as_deref(), Some("p1"));
        assert_eq!(
            ops[0].payload,
            json!({"activeStatus": "PLACEMENT_STATUS_INACTIVE"})
        );
    }
}
