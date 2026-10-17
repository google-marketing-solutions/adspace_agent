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

//! Grouping of cleaned sheet rows into CM360 entity payloads (port of the
//! `_group_*` / `_parse_row_event_tags` / `build_operations_list` functions
//! in `actions.py`).
//!
//! Payload shapes (key names, nesting and key order) match the Python
//! implementation exactly; `advertiserId` / `campaignId` are JSON strings (or
//! `null` when unknown).
//!
//! Intentional deviations from the pandas-based original: cells that pandas
//! would have turned into `NaN` (and the Python code would have stringified
//! as `"nan"`) are treated as empty strings, a numeric impression ratio
//! that is integral is rendered without decimals (`"1.0"` → `"1"`), and rows
//! without an `Ad Name` never produce an (empty-named) ad.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tracing::warn;

use super::diff::{AD_INSERT_OPERATION, EVENT_TAG_INSERT_OPERATION, Operation};
use super::sheet::{
    Row, Size, extract_site_identifier, format_date, format_date_time, parse_size, parse_weight,
};

/// Entity name → CM360 payload, in first-seen order (like a Python dict).
pub type EntityMap = IndexMap<String, Value>;

/// A single event tag parsed from the comma-separated event tag columns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventTag {
    /// Event tag name.
    pub name: String,
    /// Event tag type (upper-cased), `""` when not provided.
    #[serde(rename = "type")]
    pub tag_type: String,
    /// Event tag URL, `""` when not provided.
    pub url: String,
    /// Event tag status (upper-cased), defaults to `"ENABLED"`.
    pub status: String,
}

/// Converts an optional ID into a JSON string or `null`.
fn id_value(id: Option<&str>) -> Value {
    id.map_or(Value::Null, |v| Value::String(v.to_string()))
}

/// Serialises a [`Size`] as `{"width": w, "height": h}`.
fn size_value(size: Size) -> Value {
    json!({"width": size.width, "height": size.height})
}

/// Splits a comma-separated cell into trimmed, non-empty entries.
fn split_csv_cell(val: &str) -> Vec<&str> {
    val.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Parses comma-separated event tags from a single row.
///
/// Returns an empty list when `Event Tag Names` is missing, NA, empty or
/// `"none"` / `"nan"`. Types and statuses are upper-cased; missing entries
/// default to `""` (type / URL) and `"ENABLED"` (status).
pub fn parse_row_event_tags(row: &Row) -> Vec<EventTag> {
    let Some(raw_names) = row.get("Event Tag Names") else {
        return Vec::new();
    };
    let raw_names = raw_names.trim();
    if raw_names.is_empty() || matches!(raw_names.to_lowercase().as_str(), "none" | "nan") {
        return Vec::new();
    }
    let names = split_csv_cell(raw_names);
    if names.is_empty() {
        return Vec::new();
    }

    let raw_types = row.get_str("Event Tag Types");
    let types = split_csv_cell(&raw_types);
    let raw_urls = row.get_str("Event Tag Urls");
    let urls = split_csv_cell(&raw_urls);
    let raw_statuses = row.get_str("Event Tag Status");
    let statuses: Vec<&str> = if !raw_statuses.is_empty()
        && !matches!(raw_statuses.to_lowercase().as_str(), "none" | "nan")
    {
        split_csv_cell(&raw_statuses)
    } else {
        Vec::new()
    };

    names
        .iter()
        .enumerate()
        .map(|(i, name)| EventTag {
            name: (*name).to_string(),
            tag_type: types.get(i).map(|t| t.to_uppercase()).unwrap_or_default(),
            url: urls.get(i).map(|u| (*u).to_string()).unwrap_or_default(),
            status: statuses
                .get(i)
                .map_or_else(|| "ENABLED".to_string(), |s| s.to_uppercase()),
        })
        .collect()
}

/// Returns the creative type directly from the sheet value (trimmed), or
/// `""` when missing.
pub fn parse_creative_type(val: Option<&str>) -> String {
    val.map(str::trim).unwrap_or_default().to_string()
}

/// First non-NA cell among the given column names (canonical header first,
/// legacy aliases after it).
fn first_present<'a>(row: &'a Row, columns: &[&str]) -> Option<&'a str> {
    columns.iter().find_map(|column| row.get(column))
}

/// Groups rows into unique placements keyed by placement name.
///
/// Flight dates are read from the canonical `Pricing Schedule Start Date` /
/// `Pricing Schedule End Date` columns and the compatibility from
/// `Compatibility` (the same columns the validators check), falling back to
/// the legacy `Placement Start Date` / `Placement End Date` /
/// `Placement Type` names.
///
/// `advertiser_id` and `campaign_name` are accepted for signature parity with
/// Python but are not used in the payload.
pub fn group_placements(
    rows: &[Row],
    _advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
    _campaign_name: Option<&str>,
) -> EntityMap {
    let mut placements = EntityMap::new();
    for row in rows {
        let placement_name = row.get_str("Placement Name");
        if placements.contains_key(&placement_name) {
            continue;
        }
        let site_identifier = extract_site_identifier(row);
        let placement_start = first_present(
            row,
            &["Pricing Schedule Start Date", "Placement Start Date"],
        );
        let placement_end =
            first_present(row, &["Pricing Schedule End Date", "Placement End Date"]);
        let placement_size = row.get("Placement Size");
        let compatibility = first_present(row, &["Compatibility", "Placement Type"])
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .unwrap_or_default()
            .to_string();
        let active_status = row.get_str("Placement Status");

        let payload = json!({
            "name": placement_name,
            "campaignId": id_value(campaign_id),
            "siteId": site_identifier,
            "activeStatus": active_status,
            "size": size_value(parse_size(placement_size)),
            "compatibility": compatibility,
            "paymentSource": "",
            "pricingSchedule": {
                "startDate": format_date(placement_start),
                "endDate": format_date(placement_end),
                "pricingType": "",
            },
            "tagFormats": [],
        });
        placements.insert(placement_name, payload);
    }
    placements
}

/// Groups rows into unique creatives keyed by creative name.
pub fn group_creatives(rows: &[Row], advertiser_id: Option<&str>) -> EntityMap {
    let mut creatives = EntityMap::new();
    for row in rows {
        let Some(raw_name) = row.get("Creative Name") else {
            continue;
        };
        let creative_name = raw_name.trim();
        if creative_name.is_empty()
            || matches!(creative_name.to_lowercase().as_str(), "none" | "nan")
            || creatives.contains_key(creative_name)
        {
            continue;
        }

        let size = parse_size(row.get("Creative Dimensions"));
        let creative_type = parse_creative_type(row.get("Creative Type"));
        let rotation_val = row
            .get("Creative Rotation")
            .map(str::trim)
            .filter(|raw| !raw.is_empty() && !matches!(raw.to_lowercase().as_str(), "none" | "nan"))
            .unwrap_or("100%");

        creatives.insert(
            creative_name.to_string(),
            json!({
                "name": creative_name,
                "advertiserId": id_value(advertiser_id),
                "size": size_value(size),
                "type": creative_type,
                "rotation": rotation_val,
            }),
        );
    }
    creatives
}

/// Groups rows into unique event tags keyed by event tag name.
pub fn group_event_tags(
    rows: &[Row],
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
) -> EntityMap {
    let mut event_tags = EntityMap::new();
    for row in rows {
        for tag in parse_row_event_tags(row) {
            if event_tags.contains_key(&tag.name) {
                continue;
            }
            let payload = json!({
                "name": tag.name,
                "advertiserId": id_value(advertiser_id),
                "campaignId": id_value(campaign_id),
                "type": tag.tag_type,
                "url": tag.url,
                "status": tag.status,
            });
            event_tags.insert(tag.name, payload);
        }
    }
    event_tags
}

/// Renders the impression ratio cell: integral numbers lose their decimals
/// (`"1.0"` → `"1"`, mirroring Python's `str(int(val))` for numeric cells);
/// everything else is the trimmed string.
fn impression_ratio_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.parse::<i64>().is_ok() {
        return trimmed.to_string();
    }
    match trimmed.parse::<f64>() {
        Ok(f) if f.is_finite() && f.fract() == 0.0 => format!("{}", f as i64),
        _ => trimmed.to_string(),
    }
}

/// Returns a mutable reference to the array stored under `key` in `payload`.
fn array_mut<'a>(payload: &'a mut Map<String, Value>, key: &str) -> Option<&'a mut Vec<Value>> {
    payload.get_mut(key).and_then(Value::as_array_mut)
}

/// Groups rows into unique ads (with placement, creative and event tag
/// associations) keyed by ad name.
///
/// `campaign_name` is accepted for signature parity with Python but is not
/// used in the payload.
pub fn group_ads(
    rows: &[Row],
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
    _campaign_name: Option<&str>,
) -> EntityMap {
    let mut ads = EntityMap::new();
    for row in rows {
        let placement_name = row.get_str("Placement Name");
        let ad_name = row.get_str("Ad Name");
        if ad_name.is_empty() {
            // `validate_ad_name` rejects such rows before grouping; this guard
            // makes sure a row that slips through never becomes an
            // `ads.insert` named `""`.
            warn!(
                "Row {} has no Ad Name; skipping ad grouping for this row.",
                row.row_number
            );
            continue;
        }
        let ad_start = row.get("Ad Start Date");
        let ad_end = row.get("Ad End Date");
        let creative_id_val = row.get_str("Creative Name");

        let creative_url = Some(row.get_str("Final Trafficking URL")).filter(|u| !u.is_empty());

        let priority = row
            .get("Delivery Schedule Priority")
            .map_or_else(|| "AD_PRIORITY_01".to_string(), |p| p.trim().to_string());
        let impression_ratio = row
            .get("Delivery Schedule Impression Ratio")
            .map_or_else(|| "1".to_string(), impression_ratio_value);

        let ad_type_val = Some(row.get_str("Ad Type")).filter(|t| !t.is_empty());
        let is_click_tracker = ad_type_val.as_deref() == Some("AD_SERVING_CLICK_TRACKER");

        if !ads.contains_key(&ad_name) {
            let mut ad_payload = Map::new();
            ad_payload.insert("name".into(), json!(ad_name));
            ad_payload.insert("active".into(), json!(true));
            ad_payload.insert("advertiserId".into(), id_value(advertiser_id));
            ad_payload.insert("campaignId".into(), id_value(campaign_id));
            ad_payload.insert("type".into(), id_value(ad_type_val.as_deref()));
            ad_payload.insert("startTime".into(), json!(format_date_time(ad_start)));
            ad_payload.insert("endTime".into(), json!(format_date_time(ad_end)));
            ad_payload.insert("placementAssignments".into(), json!([]));
            ad_payload.insert("eventTagOverrides".into(), json!([]));
            if is_click_tracker {
                let val_str = row.get_str("Ad Dynamic Click Tracker").to_uppercase();
                ad_payload.insert("dynamicClickTracker".into(), json!(val_str == "TRUE"));
                if let Some(url) = &creative_url {
                    ad_payload.insert(
                        "clickThroughUrl".into(),
                        json!({"customClickThroughUrl": url}),
                    );
                }
            } else {
                ad_payload.insert(
                    "creativeRotation".into(),
                    json!({"creativeAssignments": []}),
                );
            }
            if matches!(
                ad_type_val.as_deref(),
                Some("AD_SERVING_STANDARD_AD" | "AD_SERVING_TRACKING")
            ) {
                ad_payload.insert(
                    "deliverySchedule".into(),
                    json!({"priority": priority, "impressionRatio": impression_ratio}),
                );
            }
            ads.insert(ad_name.clone(), Value::Object(ad_payload));
        }

        let Some(current_ad) = ads.get_mut(&ad_name).and_then(Value::as_object_mut) else {
            continue;
        };

        // Associate the placement by placement name.
        if let Some(assignments) = array_mut(current_ad, "placementAssignments")
            && !assignments
                .iter()
                .any(|p| p.get("placementId").and_then(Value::as_str) == Some(&placement_name))
        {
            assignments.push(json!({"placementId": placement_name, "active": true}));
        }

        // Associate the creative by creative name (non-click-tracker ads only).
        if !is_click_tracker
            && let Some(assignments) = current_ad
                .get_mut("creativeRotation")
                .and_then(Value::as_object_mut)
                .and_then(|rotation| array_mut(rotation, "creativeAssignments"))
            && !assignments
                .iter()
                .any(|c| c.get("creativeId").and_then(Value::as_str) == Some(&creative_id_val))
        {
            let weight_val = parse_weight(row.get("Creative Rotation")).max(1);
            let mut assignment = Map::new();
            assignment.insert("creativeId".into(), json!(creative_id_val));
            assignment.insert("active".into(), json!(true));
            assignment.insert("weight".into(), json!(weight_val));
            if let Some(url) = &creative_url {
                assignment.insert(
                    "clickThroughUrl".into(),
                    json!({"customClickThroughUrl": url}),
                );
            }
            assignments.push(Value::Object(assignment));
        }

        // Associate event tags with ad overrides.
        if let Some(overrides) = array_mut(current_ad, "eventTagOverrides") {
            for tag in parse_row_event_tags(row) {
                let already_present = overrides
                    .iter()
                    .any(|o| o.get("id").and_then(Value::as_str) == Some(tag.name.as_str()));
                if !already_present {
                    overrides.push(json!({"id": tag.name, "enabled": true}));
                }
            }
        }
    }
    ads
}

/// Builds the final operations list for event tags and ads: event tag
/// inserts first (in grouping order), followed by ad inserts.
pub fn build_operations_list(ads: &EntityMap, event_tags: Option<&EntityMap>) -> Vec<Operation> {
    let mut operations = Vec::new();
    if let Some(event_tags) = event_tags {
        for (tag_name, tag_payload) in event_tags {
            operations.push(Operation {
                operation: EVENT_TAG_INSERT_OPERATION.to_string(),
                id: None,
                name: tag_name.clone(),
                diff_fields: None,
                payload: tag_payload.clone(),
            });
        }
    }
    for (ad_name, ad_payload) in ads {
        operations.push(Operation {
            operation: AD_INSERT_OPERATION.to_string(),
            id: None,
            name: ad_name.clone(),
            diff_fields: None,
            payload: ad_payload.clone(),
        });
    }
    operations
}

#[cfg(test)]
mod tests {
    use super::super::sheet::test_support::{make_test_csv, row};
    use super::super::sheet::{parse_csv, prepare_data};
    use super::*;

    fn test_rows(overrides: &[(&str, &str)]) -> Vec<Row> {
        let raw = parse_csv(&make_test_csv(overrides)).unwrap();
        prepare_data(&raw).unwrap().0
    }

    #[test]
    fn test_group_placements_status_mapping() {
        let rows = vec![row(&[
            ("Placement Name", "Test Placement 1"),
            ("Placement Status", "PLACEMENT_STATUS_INACTIVE"),
            ("Compatibility", "DISPLAY"),
            ("Site ID", "123"),
            ("Pricing Schedule Type", "PRICING_TYPE_CPM"),
            ("Pricing Schedule Start Date", "8/29/3000"),
            ("Pricing Schedule End Date", "9/25/3000"),
            ("Placement Payment Source", "PLACEMENT_AGENCY_PAID"),
            ("Placement Tag Formats", "PLACEMENT_TAG_STANDARD"),
            ("Placement Size", "300x250"),
        ])];
        let placements = group_placements(&rows, Some("123"), Some("456"), None);
        assert!(placements.contains_key("Test Placement 1"));
        assert_eq!(
            placements["Test Placement 1"]["activeStatus"],
            "PLACEMENT_STATUS_INACTIVE"
        );
        // Canonical header names feed the flight dates and compatibility.
        assert_eq!(
            placements["Test Placement 1"]["pricingSchedule"],
            json!({"startDate": "3000-08-29", "endDate": "3000-09-25", "pricingType": ""})
        );
        assert_eq!(placements["Test Placement 1"]["compatibility"], "DISPLAY");
    }

    #[test]
    fn group_placements_reads_canonical_columns_from_test_csv() {
        let rows = test_rows(&[]);
        let placements = group_placements(&rows, Some("1"), Some("2"), None);
        let placement = &placements["Test~Placement~1 Test"];
        assert_eq!(placement["pricingSchedule"]["startDate"], "3000-08-29");
        assert_eq!(placement["pricingSchedule"]["endDate"], "3000-09-25");
        assert_eq!(placement["compatibility"], "DISPLAY");
        // The canonical column wins over a legacy alias when both are present.
        let rows = vec![row(&[
            ("Placement Name", "P"),
            ("Pricing Schedule Start Date", "2026-01-01"),
            ("Placement Start Date", "2025-01-01"),
            ("Pricing Schedule End Date", ""),
            ("Placement End Date", "2026-02-01"),
            ("Compatibility", ""),
            ("Placement Type", "IN_STREAM_VIDEO"),
        ])];
        let placement = &group_placements(&rows, None, None, None)["P"];
        assert_eq!(placement["pricingSchedule"]["startDate"], "2026-01-01");
        // An NA canonical cell falls back to the legacy alias.
        assert_eq!(placement["pricingSchedule"]["endDate"], "2026-02-01");
        assert_eq!(placement["compatibility"], "IN_STREAM_VIDEO");
    }

    #[test]
    fn test_cm360_actions_grouping_helpers() {
        // Legacy column names (`Placement Start Date` / `Placement End Date` /
        // `Placement Type`) are still accepted as aliases.
        let rows = vec![
            row(&[
                ("Placement Name", "P1"),
                ("Site", "SiteA"),
                ("Placement Start Date", "2026-01-01"),
                ("Placement End Date", "2026-02-01"),
                ("Placement Size", "300x250"),
                ("Placement Type", "DISPLAY"),
                ("Placement Status", "PLACEMENT_STATUS_ACTIVE"),
                ("Creative Name", "C1"),
                ("Creative Dimensions", "300x250"),
                ("Creative Type", "HTML5_BANNER"),
                ("Creative Rotation", "100%"),
                ("Event Tag Names", "ET1"),
                ("Event Tag Types", "IMPRESSION"),
                ("Event Tag Urls", "https://et1.com"),
                ("Event Tag Status", "ENABLED"),
            ]),
            row(&[
                ("Placement Name", "P1"),
                ("Site", "SiteA"),
                ("Placement Size", "300x250"),
                ("Creative Name", ""),
                ("Event Tag Names", "none"),
            ]),
        ];
        let placements = group_placements(&rows, Some("1"), Some("2"), Some("C"));
        assert_eq!(placements.len(), 1);
        assert_eq!(
            placements["P1"],
            json!({
                "name": "P1",
                "campaignId": "2",
                "siteId": "SiteA",
                "activeStatus": "PLACEMENT_STATUS_ACTIVE",
                "size": {"width": 300, "height": 250},
                "compatibility": "DISPLAY",
                "paymentSource": "",
                "pricingSchedule": {
                    "startDate": "2026-01-01",
                    "endDate": "2026-02-01",
                    "pricingType": "",
                },
                "tagFormats": [],
            })
        );
        assert_eq!(
            placements["P1"].to_string(),
            r#"{"name":"P1","campaignId":"2","siteId":"SiteA","activeStatus":"PLACEMENT_STATUS_ACTIVE","size":{"width":300,"height":250},"compatibility":"DISPLAY","paymentSource":"","pricingSchedule":{"startDate":"2026-01-01","endDate":"2026-02-01","pricingType":""},"tagFormats":[]}"#
        );

        let creatives = group_creatives(&rows, Some("1"));
        assert_eq!(creatives.len(), 1);
        assert_eq!(
            creatives["C1"],
            json!({
                "name": "C1",
                "advertiserId": "1",
                "size": {"width": 300, "height": 250},
                "type": "HTML5_BANNER",
                "rotation": "100%",
            })
        );

        let event_tags = group_event_tags(&rows, Some("1"), Some("2"));
        assert_eq!(event_tags.len(), 1);
        assert_eq!(
            event_tags["ET1"],
            json!({
                "name": "ET1",
                "advertiserId": "1",
                "campaignId": "2",
                "type": "IMPRESSION",
                "url": "https://et1.com",
                "status": "ENABLED",
            })
        );
    }

    #[test]
    fn group_placements_defaults_when_columns_missing() {
        let rows = vec![row(&[
            ("Placement Name", " P2 "),
            ("Placement Size", "bad"),
        ])];
        let placements = group_placements(&rows, None, None, None);
        assert_eq!(
            placements["P2"],
            json!({
                "name": "P2",
                "campaignId": null,
                "siteId": "",
                "activeStatus": "",
                "size": {"width": 0, "height": 0},
                "compatibility": "",
                "paymentSource": "",
                "pricingSchedule": {"startDate": null, "endDate": null, "pricingType": ""},
                "tagFormats": [],
            })
        );
    }

    #[test]
    fn group_creatives_skips_empty_and_defaults_rotation() {
        let rows = vec![
            row(&[("Creative Name", "nan")]),
            row(&[("Creative Name", "None")]),
            row(&[("Creative Name", "   ")]),
            row(&[("Placement Name", "P")]),
            row(&[("Creative Name", " C2 "), ("Creative Rotation", " nan ")]),
            row(&[("Creative Name", "C3"), ("Creative Rotation", " 50% ")]),
            row(&[("Creative Name", "C3"), ("Creative Rotation", "10%")]),
        ];
        let creatives = group_creatives(&rows, None);
        assert_eq!(creatives.keys().collect::<Vec<_>>(), vec!["C2", "C3"]);
        assert_eq!(creatives["C2"]["rotation"], "100%");
        assert_eq!(creatives["C2"]["advertiserId"], Value::Null);
        assert_eq!(creatives["C2"]["type"], "");
        assert_eq!(creatives["C3"]["rotation"], "50%");
    }

    #[test]
    fn parse_row_event_tags_branches() {
        assert_eq!(parse_row_event_tags(&row(&[])), Vec::new());
        assert_eq!(
            parse_row_event_tags(&row(&[("Event Tag Names", "")])),
            Vec::new()
        );
        assert_eq!(
            parse_row_event_tags(&row(&[("Event Tag Names", " NONE ")])),
            Vec::new()
        );
        assert_eq!(
            parse_row_event_tags(&row(&[("Event Tag Names", " , ")])),
            Vec::new()
        );
        let tags = parse_row_event_tags(&row(&[
            ("Event Tag Names", " T1 , T2, T3"),
            (
                "Event Tag Types",
                "impression_image_event_tag, click_through_event_tag",
            ),
            ("Event Tag Urls", "https://a, https://b"),
            ("Event Tag Status", "disabled"),
        ]));
        assert_eq!(
            tags,
            vec![
                EventTag {
                    name: "T1".into(),
                    tag_type: "IMPRESSION_IMAGE_EVENT_TAG".into(),
                    url: "https://a".into(),
                    status: "DISABLED".into(),
                },
                EventTag {
                    name: "T2".into(),
                    tag_type: "CLICK_THROUGH_EVENT_TAG".into(),
                    url: "https://b".into(),
                    status: "ENABLED".into(),
                },
                EventTag {
                    name: "T3".into(),
                    tag_type: String::new(),
                    url: String::new(),
                    status: "ENABLED".into(),
                },
            ]
        );
        // A "none" status cell is ignored entirely.
        let tags = parse_row_event_tags(&row(&[
            ("Event Tag Names", "T1"),
            ("Event Tag Status", "none"),
        ]));
        assert_eq!(tags[0].status, "ENABLED");
        assert_eq!(
            serde_json::to_value(&tags[0]).unwrap(),
            json!({"name": "T1", "type": "", "url": "", "status": "ENABLED"})
        );
    }

    #[test]
    fn test_parse_creative_type() {
        assert_eq!(parse_creative_type(None), "");
        assert_eq!(parse_creative_type(Some("")), "");
        assert_eq!(parse_creative_type(Some(" HTML5_BANNER ")), "HTML5_BANNER");
    }

    #[test]
    fn group_ads_from_test_csv_builds_standard_ad_payload() {
        let rows = test_rows(&[]);
        let ads = group_ads(&rows, Some("13641571"), Some("30535365"), Some("Camp"));
        assert_eq!(ads.len(), 1);
        let ad = &ads["Test Ad C"];
        assert_eq!(
            *ad,
            json!({
                "name": "Test Ad C",
                "active": true,
                "advertiserId": "13641571",
                "campaignId": "30535365",
                "type": "AD_SERVING_STANDARD_AD",
                "startTime": "3000-08-29T00:00:00Z",
                "endTime": "3000-09-25T00:00:00Z",
                "placementAssignments": [
                    {"placementId": "Test~Placement~1 Test", "active": true}
                ],
                "eventTagOverrides": [],
                "creativeRotation": {
                    "creativeAssignments": [
                        {
                            "creativeId": "TEST_ACG~300x250",
                            "active": true,
                            "weight": 100,
                            "clickThroughUrl": {
                                "customClickThroughUrl": "https://www.test.com?utm_medium=test"
                            }
                        }
                    ]
                },
                "deliverySchedule": {"priority": "AD_PRIORITY_01", "impressionRatio": "1"},
            })
        );
        // Key order matches the Python payload construction order.
        let keys: Vec<&String> = ad.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            vec![
                "name",
                "active",
                "advertiserId",
                "campaignId",
                "type",
                "startTime",
                "endTime",
                "placementAssignments",
                "eventTagOverrides",
                "creativeRotation",
                "deliverySchedule",
            ]
        );
    }

    #[test]
    fn group_ads_click_tracker_payload() {
        for (tracker_val, expected) in [("TRUE", true), ("FALSE", false), ("true", true)] {
            let rows = test_rows(&[
                ("ad_type", "AD_SERVING_CLICK_TRACKER"),
                ("ad_dynamic_click_tracker", tracker_val),
            ]);
            let ads = group_ads(&rows, Some("1"), Some("2"), None);
            let ad = &ads["Test Ad C"];
            assert_eq!(ad["dynamicClickTracker"], json!(expected));
            assert_eq!(
                ad["clickThroughUrl"],
                json!({"customClickThroughUrl": "https://www.test.com?utm_medium=test"})
            );
            assert!(ad.get("deliverySchedule").is_none());
            assert!(ad.get("creativeRotation").is_none());
            assert_eq!(ad["placementAssignments"].as_array().unwrap().len(), 1);
        }
        // Missing tracker flag / URL.
        let rows = test_rows(&[
            ("ad_type", "AD_SERVING_CLICK_TRACKER"),
            ("ad_dynamic_click_tracker", ""),
            ("final_url", ""),
        ]);
        let ad = &group_ads(&rows, None, None, None)["Test Ad C"];
        assert_eq!(ad["dynamicClickTracker"], json!(false));
        assert!(ad.get("clickThroughUrl").is_none());
    }

    #[test]
    fn group_ads_tracking_ad_has_schedule_and_rotation() {
        let rows = test_rows(&[
            ("ad_type", "AD_SERVING_TRACKING"),
            ("priority", "AD_PRIORITY_05"),
            ("impression_ratio", "2.0"),
        ]);
        let ad = &group_ads(&rows, None, None, None)["Test Ad C"];
        assert_eq!(
            ad["deliverySchedule"],
            json!({"priority": "AD_PRIORITY_05", "impressionRatio": "2"})
        );
        assert!(ad.get("creativeRotation").is_some());
        assert_eq!(ad["advertiserId"], Value::Null);
        assert_eq!(ad["type"], "AD_SERVING_TRACKING");
    }

    #[test]
    fn group_ads_brand_safe_ad_has_no_schedule_and_unknown_type_is_null() {
        let rows = test_rows(&[("ad_type", "AD_SERVING_BRAND_SAFE_AD")]);
        let ad = &group_ads(&rows, None, None, None)["Test Ad C"];
        assert!(ad.get("deliverySchedule").is_none());
        assert!(ad.get("creativeRotation").is_some());

        let rows = vec![row(&[("Placement Name", "P"), ("Ad Name", "A")])];
        let ad = &group_ads(&rows, None, None, None)["A"];
        assert_eq!(ad["type"], Value::Null);
        assert_eq!(ad["startTime"], Value::Null);
        assert_eq!(ad["endTime"], Value::Null);
        assert!(ad.get("deliverySchedule").is_none());
        // Non-click-tracker rows without a creative still get an (empty-named)
        // assignment, mirroring the Python behaviour.
        assert_eq!(
            ad["creativeRotation"]["creativeAssignments"],
            json!([{"creativeId": "", "active": true, "weight": 100}])
        );
    }

    #[test]
    fn impression_ratio_rendering() {
        assert_eq!(impression_ratio_value("1"), "1");
        assert_eq!(impression_ratio_value(" 1.0 "), "1");
        assert_eq!(impression_ratio_value("3.000"), "3");
        assert_eq!(impression_ratio_value("1.5"), "1.5");
        assert_eq!(impression_ratio_value("007"), "007");
        assert_eq!(impression_ratio_value("abc"), "abc");
        let rows = vec![row(&[
            ("Placement Name", "P"),
            ("Ad Name", "A"),
            ("Ad Type", "AD_SERVING_STANDARD_AD"),
            ("Delivery Schedule Impression Ratio", "1.0"),
            ("Delivery Schedule Priority", " AD_PRIORITY_02 "),
        ])];
        let ad = &group_ads(&rows, None, None, None)["A"];
        assert_eq!(
            ad["deliverySchedule"],
            json!({"priority": "AD_PRIORITY_02", "impressionRatio": "1"})
        );
    }

    #[test]
    fn group_ads_merges_rows_into_one_ad() {
        let rows = vec![
            row(&[
                ("Placement Name", "P1"),
                ("Ad Name", "Ad"),
                ("Ad Type", "AD_SERVING_STANDARD_AD"),
                ("Creative Name", "C1"),
                ("Creative Rotation", "0.25"),
                ("Final Trafficking URL", "https://one"),
                ("Event Tag Names", "T1, T2"),
            ]),
            row(&[
                ("Placement Name", "P2"),
                ("Ad Name", "Ad"),
                ("Ad Type", "AD_SERVING_STANDARD_AD"),
                ("Creative Name", "C2"),
                ("Creative Rotation", "0"),
                ("Event Tag Names", "T2, T3"),
            ]),
            row(&[
                ("Placement Name", "P1"),
                ("Ad Name", "Ad"),
                ("Ad Type", "AD_SERVING_STANDARD_AD"),
                ("Creative Name", "C1"),
            ]),
            // A click-tracker row of the same ad adds no creative assignment.
            row(&[
                ("Placement Name", "P3"),
                ("Ad Name", "Ad"),
                ("Ad Type", "AD_SERVING_CLICK_TRACKER"),
                ("Creative Name", "C9"),
            ]),
        ];
        let ads = group_ads(&rows, Some("1"), Some("2"), None);
        let ad = &ads["Ad"];
        assert_eq!(
            ad["placementAssignments"],
            json!([
                {"placementId": "P1", "active": true},
                {"placementId": "P2", "active": true},
                {"placementId": "P3", "active": true},
            ])
        );
        assert_eq!(
            ad["creativeRotation"]["creativeAssignments"],
            json!([
                {
                    "creativeId": "C1",
                    "active": true,
                    "weight": 25,
                    "clickThroughUrl": {"customClickThroughUrl": "https://one"}
                },
                {"creativeId": "C2", "active": true, "weight": 1},
            ])
        );
        assert_eq!(
            ad["eventTagOverrides"],
            json!([
                {"id": "T1", "enabled": true},
                {"id": "T2", "enabled": true},
                {"id": "T3", "enabled": true},
            ])
        );
    }

    #[test]
    fn group_ads_skips_rows_without_ad_name() {
        let rows = vec![
            row(&[("Placement Name", "P1"), ("Ad Name", "")]),
            row(&[("Placement Name", "P2"), ("Ad Name", "NA")]),
            row(&[("Placement Name", "P3")]),
            row(&[("Placement Name", "P4"), ("Ad Name", " A ")]),
        ];
        let ads = group_ads(&rows, None, None, None);
        assert_eq!(ads.keys().collect::<Vec<_>>(), vec!["A"]);
        assert!(!ads.contains_key(""));
    }

    #[test]
    fn test_build_operations_list() {
        let mut ads = EntityMap::new();
        ads.insert("Ad 1".into(), json!({"name": "Ad 1"}));
        ads.insert("Ad 2".into(), json!({"name": "Ad 2"}));
        let mut tags = EntityMap::new();
        tags.insert("Tag 1".into(), json!({"name": "Tag 1"}));

        let ops = build_operations_list(&ads, Some(&tags));
        assert_eq!(
            ops.iter()
                .map(|op| (op.operation.as_str(), op.name.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("dfareporting.eventTags.insert", "Tag 1"),
                ("dfareporting.ads.insert", "Ad 1"),
                ("dfareporting.ads.insert", "Ad 2"),
            ]
        );
        assert_eq!(ops[1].payload, json!({"name": "Ad 1"}));
        assert_eq!(
            serde_json::to_value(&ops[0]).unwrap(),
            json!({
                "operation": "dfareporting.eventTags.insert",
                "name": "Tag 1",
                "payload": {"name": "Tag 1"},
            })
        );
        assert_eq!(build_operations_list(&ads, None).len(), 2);
        assert!(build_operations_list(&EntityMap::new(), None).is_empty());
    }
}
