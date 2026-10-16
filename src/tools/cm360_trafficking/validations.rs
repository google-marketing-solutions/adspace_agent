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

//! Validation rules for Campaign Manager 360 trafficking sheets (port of
//! `validations.py`).
//!
//! Every validator takes a cleaned [`Row`] plus its 1-based row number and
//! returns `None` when the row passes, or a [`ValidationError`] describing the
//! first problem found. [`validate_sheet`] runs the same subset of validators,
//! in the same order, as the Python implementation. Error strings match the
//! Python ones exactly (including the Python-list rendering of the allowed
//! values, e.g. `['A', 'B']`).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use super::sheet::{Row, is_empty, parse_datetime};

/// Maximum allowed placement name length.
pub const MAX_PLACEMENT_NAME_LENGTH: usize = 512;
/// Maximum allowed ad name length.
pub const MAX_AD_NAME_LENGTH: usize = 256;

/// Allowed `Ad Type` values on insertion (`sorted(allowed)` order).
const ALLOWED_AD_TYPES: [&str; 4] = [
    "AD_SERVING_BRAND_SAFE_AD",
    "AD_SERVING_CLICK_TRACKER",
    "AD_SERVING_STANDARD_AD",
    "AD_SERVING_TRACKING",
];

/// Allowed `Placement Status` values (`sorted(allowed)` order).
const ALLOWED_PLACEMENT_STATUSES: [&str; 5] = [
    "PLACEMENT_STATUS_ACTIVE",
    "PLACEMENT_STATUS_ARCHIVED",
    "PLACEMENT_STATUS_INACTIVE",
    "PLACEMENT_STATUS_PERMANENTLY_ARCHIVED",
    "PLACEMENT_STATUS_UNKNOWN",
];

/// Allowed `Placement Tag Formats` values.
const ALLOWED_TAG_FORMATS: [&str; 15] = [
    "PLACEMENT_TAG_STANDARD",
    "PLACEMENT_TAG_IFRAME_JAVASCRIPT",
    "PLACEMENT_TAG_IFRAME_ILAYER",
    "PLACEMENT_TAG_INTERNAL_REDIRECT",
    "PLACEMENT_TAG_JAVASCRIPT",
    "PLACEMENT_TAG_INTERSTITIAL_IFRAME_JAVASCRIPT",
    "PLACEMENT_TAG_INTERSTITIAL_INTERNAL_REDIRECT",
    "PLACEMENT_TAG_INTERSTITIAL_JAVASCRIPT",
    "PLACEMENT_TAG_CLICK_COMMANDS",
    "PLACEMENT_TAG_INSTREAM_VIDEO_PREFETCH",
    "PLACEMENT_TAG_INSTREAM_VIDEO_PREFETCH_VAST_3",
    "PLACEMENT_TAG_INSTREAM_VIDEO_PREFETCH_VAST_4",
    "PLACEMENT_TAG_TRACKING",
    "PLACEMENT_TAG_TRACKING_IFRAME",
    "PLACEMENT_TAG_TRACKING_JAVASCRIPT",
];

/// Valid event tag types (`sorted(VALID_EVENT_TAG_TYPES)` order).
pub const VALID_EVENT_TAG_TYPES: [&str; 3] = [
    "CLICK_THROUGH_EVENT_TAG",
    "IMPRESSION_IMAGE_EVENT_TAG",
    "IMPRESSION_JAVASCRIPT_EVENT_TAG",
];

/// Valid event tag statuses (`sorted(VALID_EVENT_TAG_STATUSES)` order).
pub const VALID_EVENT_TAG_STATUSES: [&str; 2] = ["DISABLED", "ENABLED"];

/// Shared suffix of the event tag count-mismatch / missing-column messages.
const EVENT_TAG_COLUMNS_HINT: &str = "All 4 Event Tags Information columns ('Event Tag Names', \
     'Event Tag Types', 'Event Tag Urls', 'Event Tag Status') must have the same number of \
     comma-separated entries.";

/// A single validation failure for one sheet row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    /// 1-based row number in the original CSV.
    pub row: usize,
    /// Logical field / column the error applies to.
    pub field: String,
    /// Human-readable error message.
    pub error: String,
}

impl ValidationError {
    fn new(row: usize, field: &str, error: impl Into<String>) -> Self {
        Self {
            row,
            field: field.to_string(),
            error: error.into(),
        }
    }
}

/// Renders a list of strings like Python's `repr(list[str])`: `['A', 'B']`.
fn python_list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("'{item}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// `str(row.get("Trafficking Status", "")).strip().lower()`.
fn trafficking_status(row: &Row) -> String {
    row.get_str("Trafficking Status").to_lowercase()
}

/// `str(row.get("Ad Type", "")).strip().upper()`.
fn ad_type_upper(row: &Row) -> String {
    row.get_str("Ad Type").to_uppercase()
}

/// Splits a comma-separated cell into trimmed, non-empty entries.
fn split_csv_cell(val: &str) -> Vec<&str> {
    val.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Validates the ID field for placements: required when the trafficking
/// status is `Update`.
pub fn validate_placement_id(row: &Row, row_num: usize) -> Option<ValidationError> {
    if trafficking_status(row) == "update" && is_empty(row.get("Placement ID")) {
        return Some(ValidationError::new(
            row_num,
            "Placement ID",
            "Placement ID is required when Trafficking Status is 'Update'.",
        ));
    }
    None
}

/// Validates the Name field for placements: required and at most 512
/// characters when the trafficking status is `New`.
pub fn validate_placement_name(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status = trafficking_status(row);
    let placement_name = row.get("Placement Name");
    if status == "new" && is_empty(placement_name) {
        return Some(ValidationError::new(
            row_num,
            "Placement Name",
            "Placement Name is required when Trafficking Status is 'New'.",
        ));
    }
    if status == "new"
        && placement_name.is_some_and(|name| name.chars().count() > MAX_PLACEMENT_NAME_LENGTH)
    {
        return Some(ValidationError::new(
            row_num,
            "Placement Name",
            "Placement Name must be less than or equal to 512 characters.",
        ));
    }
    None
}

/// Validates the Profile ID (row column, falling back to the sheet metadata
/// value). Required for both `New` and `Update`.
pub fn validate_profile_id(
    row: &Row,
    row_num: usize,
    profile_id: Option<&str>,
) -> Option<ValidationError> {
    validate_required_id(row, row_num, "Profile ID", profile_id)
}

/// Validates the Advertiser ID (row column, falling back to the sheet
/// metadata value). Required for both `New` and `Update`.
pub fn validate_advertiser_id(
    row: &Row,
    row_num: usize,
    advertiser_id: Option<&str>,
) -> Option<ValidationError> {
    validate_required_id(row, row_num, "Advertiser ID", advertiser_id)
}

/// Validates the Campaign ID (row column, falling back to the sheet metadata
/// value). Required for both `New` and `Update`.
pub fn validate_campaign_id(
    row: &Row,
    row_num: usize,
    campaign_id: Option<&str>,
) -> Option<ValidationError> {
    validate_required_id(row, row_num, "Campaign ID", campaign_id)
}

/// Shared implementation of the three metadata ID validators.
fn validate_required_id(
    row: &Row,
    row_num: usize,
    field: &str,
    fallback: Option<&str>,
) -> Option<ValidationError> {
    let mut value = row.get(field);
    if is_empty(value) {
        value = fallback;
    }
    if is_empty(value) {
        return Some(ValidationError::new(
            row_num,
            field,
            format!("{field} is required."),
        ));
    }
    None
}

/// Validates the Site ID field for placements (always required).
pub fn validate_site_id(row: &Row, row_num: usize) -> Option<ValidationError> {
    if is_empty(row.get("Site ID")) {
        return Some(ValidationError::new(
            row_num,
            "Site ID",
            "Site ID is required.",
        ));
    }
    None
}

/// Validates the Payment Source field for placements: required on `New`.
pub fn validate_payment_source(row: &Row, row_num: usize) -> Option<ValidationError> {
    if trafficking_status(row) == "new" && is_empty(row.get("Placement Payment Source")) {
        return Some(ValidationError::new(
            row_num,
            "Payment Source",
            "Payment Source is required when Trafficking Status is 'New'.",
        ));
    }
    None
}

/// Validates placement compatibility on insertion (`Compatibility`, falling
/// back to `Placement Type`).
pub fn validate_compatibility(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status = trafficking_status(row);
    let mut compatibility = row.get("Compatibility");
    if is_empty(compatibility) {
        compatibility = row.get("Placement Type");
    }
    if status == "new" && is_empty(compatibility) {
        return Some(ValidationError::new(
            row_num,
            "Compatibility",
            "Compatibility is required when Trafficking Status is 'New'.",
        ));
    }
    if status == "new" {
        let val = compatibility.unwrap_or_default().trim().to_uppercase();
        if matches!(val.as_str(), "APP" | "APP_INTERSTITIAL") {
            return Some(ValidationError::new(
                row_num,
                "Compatibility",
                "APP and APP_INTERSTITIAL are no longer allowed. Use DISPLAY or \
                 DISPLAY_INTERSTITIAL instead.",
            ));
        }
        if !matches!(
            val.as_str(),
            "DISPLAY" | "DISPLAY_INTERSTITIAL" | "IN_STREAM_VIDEO" | "IN_STREAM_AUDIO"
        ) {
            return Some(ValidationError::new(
                row_num,
                "Compatibility",
                "Invalid compatibility value. Must be DISPLAY, DISPLAY_INTERSTITIAL, \
                 IN_STREAM_VIDEO, or IN_STREAM_AUDIO.",
            ));
        }
    }
    None
}

/// Validates placement size on insertion.
pub fn validate_placement_size(row: &Row, row_num: usize) -> Option<ValidationError> {
    if trafficking_status(row) == "new" && is_empty(row.get("Placement Size")) {
        return Some(ValidationError::new(
            row_num,
            "Placement Size",
            "Placement Size is required when Trafficking Status is 'New'.",
        ));
    }
    None
}

/// Validates placement pricing schedule sub-fields on insertion.
pub fn validate_pricing_schedule(row: &Row, row_num: usize) -> Option<ValidationError> {
    let mut missing: Vec<&str> = Vec::new();
    if is_empty(row.get("Pricing Schedule Start Date")) {
        missing.push("startDate");
    }
    if is_empty(row.get("Pricing Schedule End Date")) {
        missing.push("endDate");
    }
    if is_empty(row.get("Pricing Schedule Type")) {
        missing.push("pricingType");
    }
    if trafficking_status(row) == "new" && !missing.is_empty() {
        return Some(ValidationError::new(
            row_num,
            "Pricing Schedule",
            format!(
                "Pricing Schedule subfields ({}) are required on insertion.",
                missing.join(", ")
            ),
        ));
    }
    None
}

/// Validates placement tag formats on insertion.
pub fn validate_tag_formats(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status = trafficking_status(row);
    let tag_formats_val = row.get("Placement Tag Formats");
    if status == "new" && is_empty(tag_formats_val) {
        return Some(ValidationError::new(
            row_num,
            "Placement Tag Formats",
            "Placement Tag Formats is required when Trafficking Status is 'New'.",
        ));
    }
    if status == "new" {
        let formats = split_csv_cell(tag_formats_val.unwrap_or_default());
        let invalid: Vec<&str> = formats
            .into_iter()
            .filter(|f| !ALLOWED_TAG_FORMATS.contains(f))
            .collect();
        if !invalid.is_empty() {
            return Some(ValidationError::new(
                row_num,
                "Placement Tag Formats",
                format!(
                    "Invalid placement tag format values: {}.",
                    invalid.join(", ")
                ),
            ));
        }
    }
    None
}

/// Validates ad ID on edit.
pub fn validate_ad_id(row: &Row, row_num: usize) -> Option<ValidationError> {
    if trafficking_status(row) == "update" && is_empty(row.get("Ad ID")) {
        return Some(ValidationError::new(
            row_num,
            "Ad ID",
            "Ad ID is required when Trafficking Status is 'Update'.",
        ));
    }
    None
}

/// Validates ad name on insertion (required, at most 256 characters).
pub fn validate_ad_name(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status = trafficking_status(row);
    let ad_name = row.get("Ad Name");
    if status == "new" && is_empty(ad_name) {
        return Some(ValidationError::new(
            row_num,
            "Ad Name",
            "Ad Name is required when Trafficking Status is 'New'.",
        ));
    }
    if status == "new" && ad_name.is_some_and(|name| name.chars().count() > MAX_AD_NAME_LENGTH) {
        return Some(ValidationError::new(
            row_num,
            "Ad Name",
            "Ad Name must be less than or equal to 256 characters.",
        ));
    }
    None
}

/// Parses a date string safely (via [`parse_datetime`]); `None` when empty or
/// unparsable.
pub fn parse_date(val: Option<&str>) -> Option<NaiveDate> {
    if is_empty(val) {
        return None;
    }
    parse_datetime(val?.trim()).map(|dt| dt.date())
}

/// Validates the ad start date against today's local date.
pub fn validate_ad_start_time(row: &Row, row_num: usize) -> Option<ValidationError> {
    validate_ad_start_time_on(row, row_num, chrono::Local::now().date_naive())
}

/// Validates the ad start date (required, parsable, not before `today`).
pub fn validate_ad_start_time_on(
    row: &Row,
    row_num: usize,
    today: NaiveDate,
) -> Option<ValidationError> {
    let start_val = row.get("Ad Start Date");
    if is_empty(start_val) {
        return Some(ValidationError::new(
            row_num,
            "Ad Start Date",
            "Ad Start Date is required when Trafficking Status is 'New' or 'Update'.",
        ));
    }
    let raw = start_val.unwrap_or_default();
    let Some(start_date) = parse_date(start_val) else {
        return Some(ValidationError::new(
            row_num,
            "Ad Start Date",
            format!("Ad Start Date '{raw}' is not a valid date."),
        ));
    };
    if start_date < today {
        return Some(ValidationError::new(
            row_num,
            "Ad Start Date",
            format!("Ad Start Date '{raw}' cannot be in the past."),
        ));
    }
    None
}

/// Validates the ad end date (required, parsable, later than the start date
/// when the start date is present and parsable).
pub fn validate_ad_end_time(row: &Row, row_num: usize) -> Option<ValidationError> {
    let end_val = row.get("Ad End Date");
    let start_val = row.get("Ad Start Date");
    if is_empty(end_val) {
        return Some(ValidationError::new(
            row_num,
            "Ad End Date",
            "Ad End Date is required when Trafficking Status is 'New' or 'Update'.",
        ));
    }
    let end_raw = end_val.unwrap_or_default();
    let Some(end_date) = parse_date(end_val) else {
        return Some(ValidationError::new(
            row_num,
            "Ad End Date",
            format!("Ad End Date '{end_raw}' is not a valid date."),
        ));
    };
    if !is_empty(start_val)
        && parse_date(start_val).is_some_and(|start_date| end_date <= start_date)
    {
        let start_raw = start_val.unwrap_or_default();
        return Some(ValidationError::new(
            row_num,
            "Ad End Date",
            format!("Ad End Date '{end_raw}' must be later than Ad Start Date '{start_raw}'."),
        ));
    }
    None
}

/// Validates placement assignment: either a Placement ID or a Placement Name
/// must be present.
pub fn validate_placement_assignment(row: &Row, row_num: usize) -> Option<ValidationError> {
    let has_id = !is_empty(row.get("Placement ID"));
    let has_name = !is_empty(row.get("Placement Name"));
    if !(has_id || has_name) {
        return Some(ValidationError::new(
            row_num,
            "Placement ID",
            "Placement ID is required for placement assignment.",
        ));
    }
    None
}

/// Validates the ad type (required; on `New` it must be one of the allowed
/// API-creatable types).
pub fn validate_ad_type(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status = trafficking_status(row);
    let ad_type = row.get("Ad Type");
    if is_empty(ad_type) {
        return Some(ValidationError::new(
            row_num,
            "Ad Type",
            "Ad Type is required when Trafficking Status is 'New' or 'Update'.",
        ));
    }
    if status == "new" {
        let raw = ad_type.unwrap_or_default();
        let val = raw.trim().to_uppercase();
        if val == "AD_SERVING_DEFAULT_AD" {
            return Some(ValidationError::new(
                row_num,
                "Ad Type",
                "Default ads (AD_SERVING_DEFAULT_AD) cannot be created directly using the API.",
            ));
        }
        if !ALLOWED_AD_TYPES.contains(&val.as_str()) {
            return Some(ValidationError::new(
                row_num,
                "Ad Type",
                format!(
                    "Invalid Ad Type '{raw}'. Must be one of {}.",
                    python_list(&ALLOWED_AD_TYPES)
                ),
            ));
        }
    }
    None
}

/// Validates the delivery schedule sub-fields for standard ads.
pub fn validate_ad_delivery_schedule(row: &Row, row_num: usize) -> Option<ValidationError> {
    let ad_type = ad_type_upper(row);
    if !matches!(
        ad_type.as_str(),
        "AD_SERVING_STANDARD_AD" | "AD_SERVING_TRACKING"
    ) {
        return None;
    }
    if ad_type == "AD_SERVING_STANDARD_AD" {
        let mut priority = row.get("Delivery Schedule Priority");
        if is_empty(priority) {
            priority = row.get("Priority");
        }
        let mut ratio = row.get("Delivery Schedule Impression Ratio");
        if is_empty(ratio) {
            ratio = row.get("Impression Ratio");
        }
        let has_priority = !is_empty(priority);
        let has_ratio = !is_empty(ratio);
        if !(has_priority && has_ratio) {
            let mut missing: Vec<&str> = Vec::new();
            if !has_priority {
                missing.push("priority");
            }
            if !has_ratio {
                missing.push("impressionRatio");
            }
            return Some(ValidationError::new(
                row_num,
                "Delivery Schedule",
                format!(
                    "Delivery Schedule subfields ({}) are required when Ad Type is \
                     AD_SERVING_STANDARD_AD.",
                    missing.join(", ")
                ),
            ));
        }
    }
    None
}

/// Validates the dynamic click tracker flag for click tracker ads (required,
/// strictly `TRUE` or `FALSE`).
pub fn validate_ad_dynamic_click_tracker(row: &Row, row_num: usize) -> Option<ValidationError> {
    if ad_type_upper(row) != "AD_SERVING_CLICK_TRACKER" {
        return None;
    }
    let val = row.get("Ad Dynamic Click Tracker");
    if is_empty(val) {
        return Some(ValidationError::new(
            row_num,
            "Ad Dynamic Click Tracker",
            "Ad Dynamic Click Tracker is required when Ad Type is AD_SERVING_CLICK_TRACKER.",
        ));
    }
    let raw = val.unwrap_or_default();
    if !matches!(raw.trim().to_uppercase().as_str(), "TRUE" | "FALSE") {
        return Some(ValidationError::new(
            row_num,
            "Ad Dynamic Click Tracker",
            format!("Invalid Ad Dynamic Click Tracker value '{raw}'. Must be 'TRUE' or 'FALSE'."),
        ));
    }
    None
}

/// Validates the destination URL for click tracker ads.
pub fn validate_click_tracker_url(row: &Row, row_num: usize) -> Option<ValidationError> {
    if ad_type_upper(row) != "AD_SERVING_CLICK_TRACKER" {
        return None;
    }
    if is_empty(row.get("Final Trafficking URL")) {
        return Some(ValidationError::new(
            row_num,
            "Final Trafficking URL",
            "Final Trafficking URL is required for click tracker ads.",
        ));
    }
    None
}

/// Validates the creative rotation assignment for non-click-tracker ads
/// (creative name and final trafficking URL are required).
pub fn validate_creative_rotation(row: &Row, row_num: usize) -> Option<ValidationError> {
    if ad_type_upper(row) == "AD_SERVING_CLICK_TRACKER" {
        return None;
    }
    if is_empty(row.get("Creative Name")) {
        return Some(ValidationError::new(
            row_num,
            "Creative Name",
            "Creative Name is required for creative assignment.",
        ));
    }
    if is_empty(row.get("Final Trafficking URL")) {
        return Some(ValidationError::new(
            row_num,
            "Final Trafficking URL",
            "Final Trafficking URL is required for creative assignment.",
        ));
    }
    None
}

/// Validates the placement status enum (when present).
pub fn validate_placement_status(row: &Row, row_num: usize) -> Option<ValidationError> {
    let status_val = row.get("Placement Status");
    if !is_empty(status_val) {
        let raw = status_val.unwrap_or_default();
        let val = raw.trim().to_uppercase();
        if !ALLOWED_PLACEMENT_STATUSES.contains(&val.as_str()) {
            return Some(ValidationError::new(
                row_num,
                "Placement Status",
                format!(
                    "Invalid placement status '{raw}'. Must be one of {}.",
                    python_list(&ALLOWED_PLACEMENT_STATUSES)
                ),
            ));
        }
    }
    None
}

/// Validates the comma-separated event tag columns of a row: names, types,
/// URLs and statuses must have matching counts and valid values.
pub fn validate_event_tags(row: &Row, row_num: usize) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();

    let names_val = row.get("Event Tag Names");
    let types_val = row.get("Event Tag Types");
    let urls_val = row.get("Event Tag Urls");
    let status_val = row.get("Event Tag Status");

    let has_names = !is_empty(names_val);
    let has_types = !is_empty(types_val);
    let has_urls = !is_empty(urls_val);
    let has_status = !is_empty(status_val);

    if !has_names && !has_types && !has_urls && !has_status {
        return errors;
    }

    if !has_names && (has_types || has_urls || has_status) {
        errors.push(ValidationError::new(
            row_num,
            "Event Tag Names",
            "Event Tag Names is required when Event Tag Types, Urls, or Status are provided. \
             All 4 Event Tags Information columns ('Event Tag Names', 'Event Tag Types', \
             'Event Tag Urls', 'Event Tag Status') must have matching entries.",
        ));
        return errors;
    }

    let names = split_csv_cell(names_val.unwrap_or_default());
    if names.is_empty() {
        errors.push(ValidationError::new(
            row_num,
            "Event Tag Names",
            "Event Tag Names cannot be empty.",
        ));
        return errors;
    }

    if has_types {
        let types = split_csv_cell(types_val.unwrap_or_default());
        if types.len() != names.len() {
            errors.push(ValidationError::new(
                row_num,
                "Event Tag Types",
                format!(
                    "Mismatch in count of Event Tag Names ({}) and Event Tag Types ({}). {}",
                    names.len(),
                    types.len(),
                    EVENT_TAG_COLUMNS_HINT
                ),
            ));
        }
        errors.extend(
            types
                .iter()
                .filter(|t| !VALID_EVENT_TAG_TYPES.contains(&t.to_uppercase().as_str()))
                .map(|t| {
                    ValidationError::new(
                        row_num,
                        "Event Tag Types",
                        format!(
                            "Invalid Event Tag Type '{t}'. Must be one of {}.",
                            python_list(&VALID_EVENT_TAG_TYPES)
                        ),
                    )
                }),
        );
    } else {
        errors.push(ValidationError::new(
            row_num,
            "Event Tag Types",
            format!(
                "Event Tag Types is required when Event Tag Names is provided. {EVENT_TAG_COLUMNS_HINT}"
            ),
        ));
    }

    if has_urls {
        let urls = split_csv_cell(urls_val.unwrap_or_default());
        if urls.len() != names.len() {
            errors.push(ValidationError::new(
                row_num,
                "Event Tag Urls",
                format!(
                    "Mismatch in count of Event Tag Names ({}) and Event Tag Urls ({}). {}",
                    names.len(),
                    urls.len(),
                    EVENT_TAG_COLUMNS_HINT
                ),
            ));
        }
    } else {
        errors.push(ValidationError::new(
            row_num,
            "Event Tag Urls",
            format!(
                "Event Tag Urls is required when Event Tag Names is provided. {EVENT_TAG_COLUMNS_HINT}"
            ),
        ));
    }

    if has_status {
        let statuses = split_csv_cell(status_val.unwrap_or_default());
        if statuses.len() != names.len() {
            errors.push(ValidationError::new(
                row_num,
                "Event Tag Status",
                format!(
                    "Mismatch in count of Event Tag Names ({}) and Event Tag Status ({}). {}",
                    names.len(),
                    statuses.len(),
                    EVENT_TAG_COLUMNS_HINT
                ),
            ));
        }
        errors.extend(
            statuses
                .iter()
                .filter(|s| !VALID_EVENT_TAG_STATUSES.contains(&s.to_uppercase().as_str()))
                .map(|s| {
                    ValidationError::new(
                        row_num,
                        "Event Tag Status",
                        format!(
                            "Invalid Event Tag Status '{s}'. Must be one of {}.",
                            python_list(&VALID_EVENT_TAG_STATUSES)
                        ),
                    )
                }),
        );
    }

    errors
}

/// Runs all sheet validations (same validators, same order as Python) and
/// returns every error found.
///
/// Note that the placement-level validators (`validate_placement_id`,
/// `validate_placement_name`, `validate_site_id`, `validate_payment_source`,
/// `validate_compatibility`, `validate_placement_size`,
/// `validate_pricing_schedule`, `validate_tag_formats`,
/// `validate_placement_assignment`, `validate_placement_status`) are
/// intentionally NOT part of this pass, mirroring the Python implementation.
pub fn validate_sheet(
    rows: &[Row],
    profile_id: Option<&str>,
    advertiser_id: Option<&str>,
    campaign_id: Option<&str>,
) -> Vec<ValidationError> {
    let mut errors: Vec<ValidationError> = Vec::new();
    for row in rows {
        let row_num = row.row_number;
        let single_checks = [
            validate_profile_id(row, row_num, profile_id),
            validate_advertiser_id(row, row_num, advertiser_id),
            validate_campaign_id(row, row_num, campaign_id),
            validate_ad_name(row, row_num),
            validate_ad_start_time(row, row_num),
            validate_ad_end_time(row, row_num),
            validate_ad_type(row, row_num),
            validate_ad_delivery_schedule(row, row_num),
            validate_ad_dynamic_click_tracker(row, row_num),
            validate_click_tracker_url(row, row_num),
            validate_creative_rotation(row, row_num),
        ];
        errors.extend(single_checks.into_iter().flatten());
        errors.extend(validate_event_tags(row, row_num));
    }
    errors
}

#[cfg(test)]
mod tests {
    use chrono::{Days, Local};

    use super::super::sheet::test_support::{make_test_csv, row};
    use super::super::sheet::{parse_csv, prepare_data};
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Parses `make_test_csv(overrides)` and runs the full validation pass.
    fn validate_csv(overrides: &[(&str, &str)]) -> Vec<ValidationError> {
        let raw = parse_csv(&make_test_csv(overrides)).unwrap();
        let (rows, meta) = prepare_data(&raw).unwrap();
        validate_sheet(
            &rows,
            Some(&meta.profile_id),
            Some(&meta.advertiser_id),
            Some(&meta.campaign_id),
        )
    }

    #[test]
    fn test_validate_placement_name() {
        let err = validate_placement_name(
            &row(&[("Trafficking Status", "New"), ("Placement Name", "")]),
            1,
        )
        .unwrap();
        assert_eq!(err.field, "Placement Name");
        assert!(err.error.contains("Placement Name is required"));

        let long_name = "A".repeat(513);
        let err = validate_placement_name(
            &row(&[
                ("Trafficking Status", "New"),
                ("Placement Name", &long_name),
            ]),
            1,
        )
        .unwrap();
        assert!(
            err.error
                .contains("must be less than or equal to 512 characters")
        );

        let ok_name = "A".repeat(512);
        assert_eq!(
            validate_placement_name(
                &row(&[("Trafficking Status", "New"), ("Placement Name", &ok_name)]),
                1
            ),
            None
        );
        assert_eq!(
            validate_placement_name(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Placement Name", "Valid Name")
                ]),
                1
            ),
            None
        );
        assert_eq!(
            validate_placement_name(
                &row(&[("Trafficking Status", "Update"), ("Placement Name", "")]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validation_fail_missing_required_ids() {
        for (key, field) in [
            ("profile_id", "Profile ID"),
            ("advertiser_id", "Advertiser ID"),
            ("campaign_id", "Campaign ID"),
        ] {
            let raw = parse_csv(&make_test_csv(&[(key, "")])).unwrap();
            let message = prepare_data(&raw).unwrap_err().to_string();
            assert!(message.contains(field), "{message}");
        }
        // The per-row validators fall back to the metadata values.
        let raw = parse_csv(&make_test_csv(&[("campaign_id", "")])).unwrap();
        let rows = super::super::sheet::clean_sheet(&raw).unwrap();
        let errors = validate_sheet(&rows, Some("1"), Some("2"), None);
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0],
            ValidationError::new(6, "Campaign ID", "Campaign ID is required.")
        );
    }

    #[test]
    fn test_validate_placement_id() {
        assert!(
            validate_placement_id(
                &row(&[("Trafficking Status", "Update"), ("Placement ID", "")]),
                1
            )
            .is_some()
        );
        assert_eq!(
            validate_placement_id(
                &row(&[("Trafficking Status", "Update"), ("Placement ID", "12345")]),
                1
            ),
            None
        );
        assert_eq!(
            validate_placement_id(
                &row(&[("Trafficking Status", "New"), ("Placement ID", "")]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_site_id() {
        let err = validate_site_id(&row(&[("Site ID", "")]), 1).unwrap();
        assert_eq!(err.error, "Site ID is required.");
        assert_eq!(validate_site_id(&row(&[("Site ID", "12345")]), 1), None);
    }

    #[test]
    fn test_validate_payment_source() {
        assert!(
            validate_payment_source(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Placement Payment Source", "")
                ]),
                1
            )
            .is_some()
        );
        assert_eq!(
            validate_payment_source(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Placement Payment Source", "PLACEMENT_AGENCY_PAID")
                ]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_pricing_schedule() {
        let err = validate_pricing_schedule(
            &row(&[
                ("Trafficking Status", "New"),
                ("Pricing Schedule Start Date", ""),
                ("Pricing Schedule End Date", "9/25/3000"),
                ("Pricing Schedule Type", "PRICING_TYPE_CPM"),
            ]),
            1,
        )
        .unwrap();
        assert_eq!(
            err.error,
            "Pricing Schedule subfields (startDate) are required on insertion."
        );
        let err = validate_pricing_schedule(&row(&[("Trafficking Status", "New")]), 1).unwrap();
        assert_eq!(
            err.error,
            "Pricing Schedule subfields (startDate, endDate, pricingType) are required on insertion."
        );
        assert_eq!(
            validate_pricing_schedule(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Pricing Schedule Start Date", "8/29/3000"),
                    ("Pricing Schedule End Date", "9/25/3000"),
                    ("Pricing Schedule Type", "PRICING_TYPE_CPM"),
                ]),
                1
            ),
            None
        );
        assert_eq!(
            validate_pricing_schedule(&row(&[("Trafficking Status", "Update")]), 1),
            None
        );
    }

    #[test]
    fn test_validate_tag_formats() {
        assert!(
            validate_tag_formats(
                &row(&[("Trafficking Status", "New"), ("Placement Tag Formats", "")]),
                1
            )
            .is_some()
        );
        let err = validate_tag_formats(
            &row(&[
                ("Trafficking Status", "New"),
                (
                    "Placement Tag Formats",
                    "PLACEMENT_TAG_INVALID, PLACEMENT_TAG_STANDARD, OTHER",
                ),
            ]),
            1,
        )
        .unwrap();
        assert_eq!(
            err.error,
            "Invalid placement tag format values: PLACEMENT_TAG_INVALID, OTHER."
        );
        assert_eq!(
            validate_tag_formats(
                &row(&[
                    ("Trafficking Status", "New"),
                    (
                        "Placement Tag Formats",
                        "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT"
                    ),
                ]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_ad_id() {
        assert!(
            validate_ad_id(&row(&[("Trafficking Status", "Update"), ("Ad ID", "")]), 1).is_some()
        );
        assert_eq!(
            validate_ad_id(
                &row(&[("Trafficking Status", "Update"), ("Ad ID", "12345")]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_compatibility() {
        let err = validate_compatibility(
            &row(&[("Trafficking Status", "New"), ("Compatibility", "")]),
            1,
        )
        .unwrap();
        assert!(err.error.contains("Compatibility is required"));

        let err = validate_compatibility(
            &row(&[("Trafficking Status", "New"), ("Compatibility", "APP")]),
            1,
        )
        .unwrap();
        assert!(
            err.error
                .contains("APP and APP_INTERSTITIAL are no longer allowed")
        );

        let err = validate_compatibility(
            &row(&[("Trafficking Status", "New"), ("Compatibility", "INVALID")]),
            1,
        )
        .unwrap();
        assert!(err.error.contains("Invalid compatibility value"));

        assert_eq!(
            validate_compatibility(
                &row(&[("Trafficking Status", "New"), ("Compatibility", "DISPLAY")]),
                1
            ),
            None
        );
        assert_eq!(
            validate_compatibility(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Compatibility", "IN_STREAM_AUDIO")
                ]),
                1
            ),
            None
        );
        // Falls back to "Placement Type".
        assert_eq!(
            validate_compatibility(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Compatibility", ""),
                    ("Placement Type", " display ")
                ]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_placement_size() {
        let err = validate_placement_size(
            &row(&[("Trafficking Status", "New"), ("Placement Size", "")]),
            1,
        )
        .unwrap();
        assert!(err.error.contains("Placement Size is required"));
        assert_eq!(
            validate_placement_size(
                &row(&[("Trafficking Status", "New"), ("Placement Size", "300x250")]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validation_fail_missing_ad_name_on_new() {
        let errors = validate_csv(&[("ad_name", "")]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].row, 6);
        assert_eq!(errors[0].field, "Ad Name");
        assert!(errors[0].error.contains("Ad Name is required"));
    }

    #[test]
    fn test_validation_fail_long_ad_name_on_new() {
        let long_ad_name = "B".repeat(257);
        let errors = validate_csv(&[("ad_name", &long_ad_name)]);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].row, 6);
        assert_eq!(errors[0].field, "Ad Name");
        assert!(
            errors[0]
                .error
                .contains("Ad Name must be less than or equal to 256 characters")
        );
    }

    #[test]
    fn test_validation_pass_ad_start_time_today() {
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let errors = validate_csv(&[("ad_start", &today), ("ad_end", "8/29/3000")]);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_validation_fail_ad_dates() {
        let yesterday = (Local::now().date_naive() - Days::new(1))
            .format("%Y-%m-%d")
            .to_string();
        let cases: Vec<(&str, &str, &str, &str, &str)> = vec![
            (
                "New",
                &yesterday,
                "8/29/3000",
                "Ad Start Date",
                "cannot be in the past",
            ),
            (
                "Update",
                &yesterday,
                "8/29/3000",
                "Ad Start Date",
                "cannot be in the past",
            ),
            (
                "New",
                "8/29/3000",
                "8/28/3000",
                "Ad End Date",
                "must be later than Ad Start Date",
            ),
            (
                "Update",
                "8/29/3000",
                "8/28/3000",
                "Ad End Date",
                "must be later than Ad Start Date",
            ),
            (
                "New",
                "invalid-date",
                "8/29/3000",
                "Ad Start Date",
                "is not a valid date",
            ),
        ];
        for (status, ad_start, ad_end, expected_field, expected_err) in cases {
            let is_update = status == "Update";
            let errors = validate_csv(&[
                ("status", status),
                ("ad_start", ad_start),
                ("ad_end", ad_end),
                ("ad_id", if is_update { "9999" } else { "" }),
                ("placement_id", if is_update { "8888" } else { "" }),
            ]);
            assert_eq!(errors.len(), 1, "{status} {ad_start} {ad_end}: {errors:?}");
            assert_eq!(errors[0].field, expected_field);
            assert!(
                errors[0].error.contains(expected_err),
                "{}",
                errors[0].error
            );
        }
    }

    #[test]
    fn validate_ad_start_time_on_fixed_today() {
        let today = ymd(2026, 6, 15);
        let err = validate_ad_start_time_on(&row(&[("Ad Start Date", "")]), 1, today).unwrap();
        assert_eq!(
            err.error,
            "Ad Start Date is required when Trafficking Status is 'New' or 'Update'."
        );
        let err = validate_ad_start_time_on(&row(&[("Ad Start Date", " 2026-06-14 ")]), 1, today)
            .unwrap();
        assert_eq!(
            err.error,
            "Ad Start Date ' 2026-06-14 ' cannot be in the past."
        );
        let err = validate_ad_start_time_on(&row(&[("Ad Start Date", "nope")]), 1, today).unwrap();
        assert_eq!(err.error, "Ad Start Date 'nope' is not a valid date.");
        assert_eq!(
            validate_ad_start_time_on(&row(&[("Ad Start Date", "6/15/2026")]), 1, today),
            None
        );
        assert_eq!(
            validate_ad_start_time_on(&row(&[("Ad Start Date", "2026-06-16T00:00:00Z")]), 1, today),
            None
        );
        // Real clock: far future passes, far past fails.
        assert_eq!(
            validate_ad_start_time(&row(&[("Ad Start Date", "8/29/3000")]), 1),
            None
        );
        assert!(validate_ad_start_time(&row(&[("Ad Start Date", "2000-01-01")]), 1).is_some());
    }

    #[test]
    fn validate_ad_end_time_rules() {
        let err = validate_ad_end_time(&row(&[("Ad End Date", "")]), 1).unwrap();
        assert_eq!(
            err.error,
            "Ad End Date is required when Trafficking Status is 'New' or 'Update'."
        );
        let err = validate_ad_end_time(&row(&[("Ad End Date", "bad")]), 1).unwrap();
        assert_eq!(err.error, "Ad End Date 'bad' is not a valid date.");
        let err = validate_ad_end_time(
            &row(&[("Ad Start Date", "8/29/3000"), ("Ad End Date", "8/29/3000")]),
            1,
        )
        .unwrap();
        assert_eq!(
            err.error,
            "Ad End Date '8/29/3000' must be later than Ad Start Date '8/29/3000'."
        );
        assert_eq!(
            validate_ad_end_time(
                &row(&[("Ad Start Date", "8/29/3000"), ("Ad End Date", "8/30/3000")]),
                1
            ),
            None
        );
        // Unparsable start date is ignored by the end-date check.
        assert_eq!(
            validate_ad_end_time(
                &row(&[("Ad Start Date", "bad"), ("Ad End Date", "8/30/3000")]),
                1
            ),
            None
        );
    }

    #[test]
    fn test_validate_placement_assignment() {
        let err =
            validate_placement_assignment(&row(&[("Placement ID", ""), ("Placement Name", "")]), 1)
                .unwrap();
        assert_eq!(err.field, "Placement ID");
        assert!(err.error.contains("Placement ID is required"));
        assert_eq!(
            validate_placement_assignment(&row(&[("Placement Name", "Test Placement")]), 1),
            None
        );
        assert_eq!(
            validate_placement_assignment(&row(&[("Placement ID", "12345")]), 1),
            None
        );
    }

    #[test]
    fn test_validation_fail_missing_ad_type_on_new() {
        let errors = validate_csv(&[("ad_type", "")]);
        let ad_type_error = errors.iter().find(|e| e.field == "Ad Type").unwrap();
        assert!(ad_type_error.error.contains("Ad Type is required"));
    }

    #[test]
    fn test_validation_fail_forbidden_ad_type_on_new() {
        let errors = validate_csv(&[("ad_type", "AD_SERVING_DEFAULT_AD")]);
        let ad_type_error = errors.iter().find(|e| e.field == "Ad Type").unwrap();
        assert!(ad_type_error.error.contains("cannot be created directly"));
    }

    #[test]
    fn validate_ad_type_invalid_value_renders_python_list() {
        let err = validate_ad_type(
            &row(&[("Trafficking Status", "New"), ("Ad Type", " AD_FOO ")]),
            3,
        )
        .unwrap();
        assert_eq!(
            err.error,
            "Invalid Ad Type ' AD_FOO '. Must be one of ['AD_SERVING_BRAND_SAFE_AD', \
             'AD_SERVING_CLICK_TRACKER', 'AD_SERVING_STANDARD_AD', 'AD_SERVING_TRACKING']."
        );
        // Not checked on update.
        assert_eq!(
            validate_ad_type(
                &row(&[("Trafficking Status", "Update"), ("Ad Type", "AD_FOO")]),
                3
            ),
            None
        );
        assert_eq!(
            validate_ad_type(
                &row(&[
                    ("Trafficking Status", "New"),
                    ("Ad Type", "ad_serving_tracking")
                ]),
                3
            ),
            None
        );
    }

    #[test]
    fn test_validation_fail_missing_delivery_schedule() {
        for status in ["New", "Update"] {
            let is_update = status == "Update";
            let errors = validate_csv(&[
                ("status", status),
                ("ad_type", "AD_SERVING_STANDARD_AD"),
                ("priority", ""),
                ("ad_id", if is_update { "9999" } else { "" }),
                ("placement_id", if is_update { "8888" } else { "" }),
            ]);
            let schedule_error = errors
                .iter()
                .find(|e| e.field == "Delivery Schedule")
                .unwrap();
            assert_eq!(
                schedule_error.error,
                "Delivery Schedule subfields (priority) are required when Ad Type is \
                 AD_SERVING_STANDARD_AD."
            );
        }
        let err = validate_ad_delivery_schedule(&row(&[("Ad Type", "AD_SERVING_STANDARD_AD")]), 1)
            .unwrap();
        assert!(err.error.contains("(priority, impressionRatio)"));
        // Legacy column names are accepted as fallbacks.
        assert_eq!(
            validate_ad_delivery_schedule(
                &row(&[
                    ("Ad Type", "AD_SERVING_STANDARD_AD"),
                    ("Priority", "AD_PRIORITY_01"),
                    ("Impression Ratio", "1"),
                ]),
                1
            ),
            None
        );
        // Tracking ads never require a schedule; other types are ignored.
        assert_eq!(
            validate_ad_delivery_schedule(&row(&[("Ad Type", "AD_SERVING_TRACKING")]), 1),
            None
        );
        assert_eq!(
            validate_ad_delivery_schedule(&row(&[("Ad Type", "AD_SERVING_CLICK_TRACKER")]), 1),
            None
        );
    }

    #[test]
    fn test_validation_fail_missing_creative_identifier() {
        let errors = validate_csv(&[("creative_id", ""), ("creative_name", "")]);
        let creative_error = errors.iter().find(|e| e.field == "Creative Name").unwrap();
        assert!(creative_error.error.contains("Creative Name is required"));
    }

    #[test]
    fn test_validation_fail_missing_click_through_url() {
        let errors = validate_csv(&[("final_url", "")]);
        let url_error = errors
            .iter()
            .find(|e| e.field == "Final Trafficking URL")
            .unwrap();
        assert!(
            url_error
                .error
                .contains("Final Trafficking URL is required")
        );
        assert_eq!(
            url_error.error,
            "Final Trafficking URL is required for creative assignment."
        );
    }

    #[test]
    fn test_validate_placement_status() {
        let err =
            validate_placement_status(&row(&[("Placement Status", "PLACEMENT_STATUS_INVALID")]), 1)
                .unwrap();
        assert_eq!(err.field, "Placement Status");
        assert_eq!(
            err.error,
            "Invalid placement status 'PLACEMENT_STATUS_INVALID'. Must be one of \
             ['PLACEMENT_STATUS_ACTIVE', 'PLACEMENT_STATUS_ARCHIVED', 'PLACEMENT_STATUS_INACTIVE', \
             'PLACEMENT_STATUS_PERMANENTLY_ARCHIVED', 'PLACEMENT_STATUS_UNKNOWN']."
        );
        assert_eq!(
            validate_placement_status(&row(&[("Placement Status", "PLACEMENT_STATUS_ACTIVE")]), 1),
            None
        );
        assert_eq!(
            validate_placement_status(
                &row(&[("Placement Status", "PLACEMENT_STATUS_PERMANENTLY_ARCHIVED")]),
                1
            ),
            None
        );
        assert_eq!(validate_placement_status(&row(&[]), 1), None);
    }

    #[test]
    fn test_validation_click_tracker_success() {
        for (status, tracker_val) in [("New", "TRUE"), ("Update", "FALSE")] {
            let is_update = status == "Update";
            let errors = validate_csv(&[
                ("status", status),
                ("ad_id", if is_update { "123456" } else { "" }),
                ("placement_id", if is_update { "789101" } else { "" }),
                ("ad_type", "AD_SERVING_CLICK_TRACKER"),
                ("ad_dynamic_click_tracker", tracker_val),
            ]);
            assert!(errors.is_empty(), "{errors:?}");
        }
    }

    #[test]
    fn test_validation_click_tracker_fail_missing_value() {
        for status in ["New", "Update"] {
            let is_update = status == "Update";
            let errors = validate_csv(&[
                ("status", status),
                ("ad_id", if is_update { "123456" } else { "" }),
                ("placement_id", if is_update { "789101" } else { "" }),
                ("ad_type", "AD_SERVING_CLICK_TRACKER"),
                ("ad_dynamic_click_tracker", ""),
            ]);
            let err = errors
                .iter()
                .find(|e| e.field == "Ad Dynamic Click Tracker")
                .unwrap();
            assert!(err.error.contains("Ad Dynamic Click Tracker is required"));
        }
    }

    #[test]
    fn test_validation_click_tracker_fail_invalid_boolean() {
        let errors = validate_csv(&[
            ("ad_type", "AD_SERVING_CLICK_TRACKER"),
            ("ad_dynamic_click_tracker", "1"),
        ]);
        let err = errors
            .iter()
            .find(|e| e.field == "Ad Dynamic Click Tracker")
            .unwrap();
        assert_eq!(
            err.error,
            "Invalid Ad Dynamic Click Tracker value '1'. Must be 'TRUE' or 'FALSE'."
        );
    }

    #[test]
    fn validate_click_tracker_url_rules() {
        let err = validate_click_tracker_url(&row(&[("Ad Type", "AD_SERVING_CLICK_TRACKER")]), 1)
            .unwrap();
        assert_eq!(
            err.error,
            "Final Trafficking URL is required for click tracker ads."
        );
        assert_eq!(
            validate_click_tracker_url(
                &row(&[
                    ("Ad Type", "ad_serving_click_tracker"),
                    ("Final Trafficking URL", "https://x.test")
                ]),
                1
            ),
            None
        );
        assert_eq!(
            validate_click_tracker_url(&row(&[("Ad Type", "AD_SERVING_STANDARD_AD")]), 1),
            None
        );
    }

    #[test]
    fn test_validate_profile_id() {
        assert_eq!(
            validate_profile_id(&row(&[("Profile ID", "12345")]), 1, None),
            None
        );
        assert_eq!(validate_profile_id(&row(&[]), 1, Some("12345")), None);
        let err = validate_profile_id(&row(&[]), 1, None).unwrap();
        assert_eq!(err.field, "Profile ID");
        assert_eq!(err.error, "Profile ID is required.");
        let err = validate_advertiser_id(&row(&[("Advertiser ID", "nan")]), 2, Some("")).unwrap();
        assert_eq!(
            err,
            ValidationError::new(2, "Advertiser ID", "Advertiser ID is required.")
        );
        assert_eq!(validate_campaign_id(&row(&[]), 1, Some("1")), None);
    }

    #[test]
    fn test_validate_event_tags_all_branches() {
        assert_eq!(validate_event_tags(&row(&[]), 1), Vec::new());

        let err = validate_event_tags(&row(&[("Event Tag Types", "IMPRESSION")]), 1);
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].field, "Event Tag Names");
        assert!(err[0].error.starts_with("Event Tag Names is required when"));

        let err_empty_names = validate_event_tags(&row(&[("Event Tag Names", " , ")]), 1);
        assert_eq!(err_empty_names.len(), 1);
        assert!(err_empty_names[0].error.contains("cannot be empty"));

        let err_missing = validate_event_tags(&row(&[("Event Tag Names", "Tag1")]), 1);
        assert_eq!(err_missing.len(), 2);
        assert_eq!(err_missing[0].field, "Event Tag Types");
        assert_eq!(
            err_missing[0].error,
            "Event Tag Types is required when Event Tag Names is provided. All 4 Event Tags \
             Information columns ('Event Tag Names', 'Event Tag Types', 'Event Tag Urls', \
             'Event Tag Status') must have the same number of comma-separated entries."
        );
        assert_eq!(err_missing[1].field, "Event Tag Urls");

        let err_mismatch = validate_event_tags(
            &row(&[
                ("Event Tag Names", "Tag1, Tag2"),
                ("Event Tag Types", "INVALID_TYPE"),
                ("Event Tag Urls", "https://a.com"),
                ("Event Tag Status", "INVALID_STATUS"),
            ]),
            1,
        );
        assert_eq!(err_mismatch.len(), 5);
        assert_eq!(
            err_mismatch[0].error,
            "Mismatch in count of Event Tag Names (2) and Event Tag Types (1). All 4 Event Tags \
             Information columns ('Event Tag Names', 'Event Tag Types', 'Event Tag Urls', \
             'Event Tag Status') must have the same number of comma-separated entries."
        );
        assert_eq!(
            err_mismatch[1].error,
            "Invalid Event Tag Type 'INVALID_TYPE'. Must be one of ['CLICK_THROUGH_EVENT_TAG', \
             'IMPRESSION_IMAGE_EVENT_TAG', 'IMPRESSION_JAVASCRIPT_EVENT_TAG']."
        );
        assert!(
            err_mismatch[2]
                .error
                .starts_with("Mismatch in count of Event Tag Names (2) and Event Tag Urls (1).")
        );
        assert!(
            err_mismatch[3]
                .error
                .starts_with("Mismatch in count of Event Tag Names (2) and Event Tag Status (1).")
        );
        assert_eq!(
            err_mismatch[4].error,
            "Invalid Event Tag Status 'INVALID_STATUS'. Must be one of ['DISABLED', 'ENABLED']."
        );

        let valid = validate_event_tags(
            &row(&[
                ("Event Tag Names", "Tag1"),
                ("Event Tag Types", "impression_image_event_tag"),
                ("Event Tag Urls", "https://a.com"),
                ("Event Tag Status", "enabled"),
            ]),
            1,
        );
        assert_eq!(valid, Vec::new());
    }

    #[test]
    fn validate_sheet_runs_validators_in_python_order() {
        let rows = vec![super::super::sheet::test_support::row_numbered(
            7,
            &[
                ("Trafficking Status", "New"),
                ("Ad Name", ""),
                ("Ad Start Date", ""),
                ("Ad End Date", ""),
                ("Ad Type", "AD_SERVING_STANDARD_AD"),
                ("Creative Name", ""),
                ("Event Tag Names", "T1"),
            ],
        )];
        let errors = validate_sheet(&rows, None, Some("2"), Some("3"));
        let fields: Vec<&str> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(
            fields,
            vec![
                "Profile ID",
                "Ad Name",
                "Ad Start Date",
                "Ad End Date",
                "Delivery Schedule",
                "Creative Name",
                "Event Tag Types",
                "Event Tag Urls",
            ]
        );
        assert!(errors.iter().all(|e| e.row == 7));
        assert_eq!(
            serde_json::to_value(&errors[0]).unwrap(),
            serde_json::json!({"row": 7, "field": "Profile ID", "error": "Profile ID is required."})
        );
    }

    #[test]
    fn full_test_csv_passes_validation() {
        assert_eq!(validate_csv(&[]), Vec::new());
    }
}
