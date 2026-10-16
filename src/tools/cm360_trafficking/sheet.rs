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

//! Sheet loading, header detection, cleaning and metadata extraction for the
//! CM360 trafficking tools (port of `utilities.py`).
//!
//! The pandas `DataFrame` used by the Python implementation is replaced by a
//! [`RawSheet`] — a grid of raw string cells parsed with "no header"
//! semantics — and a list of cleaned [`Row`]s keyed by header name. pandas'
//! NA semantics are reproduced by [`is_na`] / [`is_empty`].
//!
//! Intentional deviations from the pandas-based original (all cases where
//! pandas would either crash or produce clearly wrong output):
//!
//! - header names are trimmed, and empty / NA header cells yield no column;
//! - ragged CSV rows are accepted (missing cells behave like NA cells);
//! - [`format_date`] returns `None` instead of raising `IndexError` for
//!   whitespace-only input;
//! - [`to_csv`] preserves the original text of NA-like cells (pandas would
//!   write them back as empty strings).

use std::collections::HashSet;

use anyhow::{Context, anyhow, bail};
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Raw CSV grid: rows × cells, rows may be ragged.
pub type RawSheet = Vec<Vec<String>>;

/// Number of leading "global metadata" rows that are skipped by the first
/// header-detection pass.
pub const GLOBAL_METADATA_ROWS_COUNT: usize = 2;

/// pandas' default `na_values` (compared after trimming, case-sensitive).
const PANDAS_NA_VALUES: [&str; 19] = [
    "", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan", "1.#IND", "1.#QNAN",
    "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a", "nan", "null",
];

/// Header cell values that identify the table header row.
const HEADER_MARKERS: [&str; 3] = ["Trafficking Status", "Placement Name", "Ad Name"];

/// True when a raw cell would be NaN under pandas' default `na_values`:
/// "", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan",
/// "1.#IND", "1.#QNAN", "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a",
/// "nan", "null" (compared after trimming; case-sensitive like pandas).
pub fn is_na(cell: &str) -> bool {
    PANDAS_NA_VALUES.contains(&cell.trim())
}

/// Port of `_is_empty`: `None`, NA, or trimmed-lowercase in
/// `{"", "none", "nan"}`.
pub fn is_empty(val: Option<&str>) -> bool {
    match val {
        None => true,
        Some(v) => is_na(v) || matches!(v.trim().to_lowercase().as_str(), "" | "none" | "nan"),
    }
}

/// A cleaned data row of the trafficking sheet.
///
/// Cells are stored as the raw (untrimmed) strings read from the CSV, keyed
/// by the trimmed header name. NA handling happens in [`Row::get`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 1-based line number of this row in the ORIGINAL CSV (raw index + 1),
    /// i.e. the Python `Row Number` column.
    pub row_number: usize,
    cells: IndexMap<String, String>,
}

impl Row {
    /// Creates a row from its 1-based row number and header → raw cell map.
    pub fn new(row_number: usize, cells: IndexMap<String, String>) -> Self {
        Self { row_number, cells }
    }

    /// Column exists AND the cell is not NA → `Some(raw cell, untrimmed)`.
    pub fn get(&self, column: &str) -> Option<&str> {
        self.cells
            .get(column)
            .map(String::as_str)
            .filter(|cell| !is_na(cell))
    }

    /// `str(row.get(col, "")).strip()` convenience: trimmed value or `""`.
    ///
    /// Unlike Python (where a NaN cell stringifies to `"nan"`), NA cells
    /// yield `""`.
    pub fn get_str(&self, column: &str) -> String {
        self.get(column)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    }

    /// True when the sheet has a column with this (trimmed) header name.
    pub fn has_column(&self, column: &str) -> bool {
        self.cells.contains_key(column)
    }

    /// Iterates over the column names of this row in sheet order.
    pub fn columns(&self) -> impl Iterator<Item = &str> {
        self.cells.keys().map(String::as_str)
    }
}

/// Parse CSV text with no header row (flexible/ragged rows allowed).
///
/// Completely blank lines are skipped, mirroring pandas'
/// `skip_blank_lines=True`; a leading UTF-8 BOM is ignored.
pub fn parse_csv(content: &str) -> anyhow::Result<RawSheet> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(content.as_bytes());

    let mut rows: RawSheet = Vec::new();
    for record in reader.records() {
        let record = record.context("Failed to parse CSV content")?;
        let cells: Vec<String> = record.iter().map(str::to_string).collect();
        if cells.is_empty() || (cells.len() == 1 && cells[0].is_empty()) {
            // Blank line (pandas `skip_blank_lines=True`).
            continue;
        }
        rows.push(cells);
    }
    Ok(rows)
}

/// Serialise a [`RawSheet`] back to CSV (no header, no index), minimal
/// quoting, `"\n"` line endings.
pub fn to_csv(raw: &RawSheet) -> anyhow::Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_writer(Vec::new());
    for row in raw {
        writer
            .write_record(row)
            .context("Failed to serialise CSV row")?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|e| anyhow!("Failed to flush CSV writer: {e}"))?;
    String::from_utf8(bytes).context("CSV output was not valid UTF-8")
}

/// Width/height pair parsed from a size string such as `"300x250"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    /// Width in pixels (0 when unknown).
    pub width: i64,
    /// Height in pixels (0 when unknown).
    pub height: i64,
}

/// Parses a size string like `"300x250"` into a [`Size`].
///
/// Spaces are removed and the value is lower-cased before splitting on `x`;
/// anything that does not yield exactly two integers becomes `0x0`.
pub fn parse_size(val: Option<&str>) -> Size {
    let zero = Size {
        width: 0,
        height: 0,
    };
    let Some(val) = val else {
        return zero;
    };
    let normalised = val.to_lowercase().replace(' ', "");
    let parts: Vec<&str> = normalised.split('x').collect();
    if parts.len() != 2 {
        return zero;
    }
    match (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
        (Ok(width), Ok(height)) => Size { width, height },
        _ => zero,
    }
}

/// Date-time formats tried (in order) after the RFC 3339 parser.
const NAIVE_DATETIME_FORMATS: [&str; 14] = [
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%m/%d/%y %H:%M:%S",
    "%m/%d/%y %H:%M",
    "%m/%d/%Y %H:%M:%S",
    "%m/%d/%Y %H:%M",
    "%m/%d/%Y %I:%M:%S %p",
    "%m/%d/%Y %I:%M %p",
    "%Y-%m-%d %I:%M:%S %p",
    "%Y-%m-%d %I:%M %p",
];

/// Date-time formats with a numeric UTC offset (`+0500` / `+05:00`).
const OFFSET_DATETIME_FORMATS: [&str; 4] = [
    "%Y-%m-%dT%H:%M:%S%.f%z",
    "%Y-%m-%d %H:%M:%S%.f%z",
    "%Y-%m-%dT%H:%M%z",
    "%Y-%m-%d %H:%M%z",
];

/// Date-only formats tried (in order) when no time component is present.
const NAIVE_DATE_FORMATS: [&str; 16] = [
    "%Y-%m-%d",
    "%Y/%m/%d",
    "%m/%d/%y",
    "%m/%d/%Y",
    "%m-%d-%Y",
    "%d-%b-%Y",
    "%d-%B-%Y",
    "%b %d, %Y",
    "%B %d, %Y",
    "%b %d %Y",
    "%B %d %Y",
    "%d %b %Y",
    "%d %B %Y",
    "%d %b, %Y",
    "%d %B, %Y",
    "%Y%m%d",
];

/// Flexible date/time parsing standing in for `pd.to_datetime(str(val))`.
///
/// Accepts RFC 3339 (`"2026-09-07T00:00:00Z"`, with fractional seconds and
/// offsets — the offset is ignored and the wall-clock part is returned),
/// `"%Y-%m-%d"`, `"%Y-%m-%d %H:%M:%S"`, `"%Y-%m-%dT%H:%M:%S"`, `"%Y/%m/%d"`,
/// `"%m/%d/%Y"`, `"%m/%d/%y"`, `"%m/%d/%Y %H:%M"`, `"%m/%d/%Y %H:%M:%S"`,
/// `"%d-%b-%Y"`, `"%b %d, %Y"`, `"%B %d, %Y"`, `"%d %B %Y"`, `"%Y%m%d"` and a
/// few close variants. Returns `None` for anything else (including empty or
/// whitespace-only input).
pub fn parse_datetime(val: &str) -> Option<NaiveDateTime> {
    let s = val.trim();
    if s.is_empty() {
        return None;
    }

    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_local());
    }
    for format in OFFSET_DATETIME_FORMATS {
        if let Ok(dt) = DateTime::parse_from_str(s, format) {
            return Some(dt.naive_local());
        }
    }

    // "2026-06-15 12:30:00Z" style: a trailing Zulu marker without RFC 3339
    // shape — treat as wall-clock.
    let without_zulu = s.strip_suffix(['Z', 'z']).unwrap_or(s);
    for format in NAIVE_DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(without_zulu, format) {
            return Some(dt);
        }
    }
    for format in NAIVE_DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(s, format) {
            return date.and_hms_opt(0, 0, 0);
        }
    }
    None
}

/// Formats date values into a standard `YYYY-MM-DD` string.
///
/// Falls back to the first whitespace-separated token of the raw value when
/// it cannot be parsed (Python: `str(val).split()[0]`); returns `None` for
/// missing or whitespace-only values (Python would raise `IndexError`).
pub fn format_date(val: Option<&str>) -> Option<String> {
    let raw = val?;
    if let Some(parsed) = parse_datetime(raw) {
        return Some(parsed.format("%Y-%m-%d").to_string());
    }
    raw.split_whitespace().next().map(str::to_string)
}

/// Formats datetime values into an ISO-8601 `YYYY-MM-DDTHH:MM:SSZ` string.
///
/// Falls back to the raw string when it cannot be parsed; `None` when missing.
pub fn format_date_time(val: Option<&str>) -> Option<String> {
    let raw = val?;
    match parse_datetime(raw) {
        Some(parsed) => Some(parsed.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        None => Some(raw.to_string()),
    }
}

/// Normalizes datetime values into a standard UTC ISO-8601 string.
///
/// Returns `None` for missing values and for `""` / `"none"` / `"nan"` /
/// `"null"` (case-insensitive, after trimming); falls back to the trimmed raw
/// string when it cannot be parsed.
pub fn normalize_iso_datetime(val: Option<&str>) -> Option<String> {
    let val_str = val?.trim();
    if matches!(
        val_str.to_lowercase().as_str(),
        "" | "none" | "nan" | "null"
    ) {
        return None;
    }
    match parse_datetime(val_str) {
        Some(parsed) => Some(parsed.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        None => Some(val_str.to_string()),
    }
}

/// Returns the placement `activeStatus` enum string directly from the sheet
/// (trimmed), or `""` when missing.
pub fn normalize_placement_status(val: Option<&str>) -> String {
    val.map(str::trim).unwrap_or_default().to_string()
}

/// Parses creative rotation weights (multiplies decimals by 100).
///
/// A `%` sign is stripped; values `<= 1.0` are treated as fractions and
/// multiplied by 100; anything unparsable (or missing) yields `100`.
pub fn parse_weight(val: Option<&str>) -> i64 {
    let Some(val) = val else {
        return 100;
    };
    let cleaned = val.trim().replace('%', "");
    match cleaned.parse::<f64>() {
        Ok(f_val) if f_val.is_finite() => {
            if f_val <= 1.0 {
                (f_val * 100.0) as i64
            } else {
                f_val as i64
            }
        }
        _ => 100,
    }
}

/// Extracts the trimmed `Site` column value from a row, or `""`.
pub fn extract_site_identifier(row: &Row) -> String {
    row.get_str("Site")
}

/// True when the row's trimmed, non-NA cells contain one of the header
/// marker values.
fn is_header_row(row: &[String]) -> bool {
    row.iter()
        .filter(|cell| !is_na(cell))
        .any(|cell| HEADER_MARKERS.contains(&cell.trim()))
}

/// Two-pass header search exactly like Python: first rows with index >=
/// [`GLOBAL_METADATA_ROWS_COUNT`] whose trimmed non-NA cells contain
/// `"Trafficking Status"` or `"Placement Name"` or `"Ad Name"`; then any row.
pub fn find_header_row(raw: &RawSheet) -> Option<usize> {
    raw.iter()
        .enumerate()
        .skip(GLOBAL_METADATA_ROWS_COUNT)
        .find(|(_, row)| is_header_row(row))
        .map(|(idx, _)| idx)
        .or_else(|| raw.iter().position(|row| is_header_row(row)))
}

/// Port of `clean_dataframe`.
///
/// Steps: locate the header row → derive column names (trimmed; empty/NA
/// header cells get no column, the first occurrence of a duplicate name wins)
/// → take the data rows after the header → drop rows where every cell is NA →
/// drop rows where all of the present `"Placement Name"` / `"Ad Name"` /
/// `"Creative Name"` columns are NA → set the 1-based row number → if a
/// `"Trafficking Status"` column exists keep only rows whose trimmed
/// lowercase status is `"new"` or `"update"`.
///
/// # Errors
///
/// `"Could not find table header in the sheet."` when no header row exists.
pub fn clean_sheet(raw: &RawSheet) -> anyhow::Result<Vec<Row>> {
    let Some(header_idx) = find_header_row(raw) else {
        bail!("Could not find table header in the sheet.");
    };

    let mut columns: Vec<(String, usize)> = Vec::new();
    for (col_idx, cell) in raw[header_idx].iter().enumerate() {
        let name = cell.trim();
        if name.is_empty() || is_na(cell) || columns.iter().any(|(n, _)| n == name) {
            continue;
        }
        columns.push((name.to_string(), col_idx));
    }

    let critical_columns: Vec<&str> = ["Placement Name", "Ad Name", "Creative Name"]
        .into_iter()
        .filter(|wanted| columns.iter().any(|(name, _)| name == wanted))
        .collect();
    let has_status_column = columns.iter().any(|(name, _)| name == "Trafficking Status");

    let mut rows = Vec::new();
    for (raw_idx, raw_row) in raw.iter().enumerate().skip(header_idx + 1) {
        if raw_row.iter().all(|cell| is_na(cell)) {
            continue;
        }
        let cells: IndexMap<String, String> = columns
            .iter()
            .map(|(name, col_idx)| {
                (
                    name.clone(),
                    raw_row.get(*col_idx).cloned().unwrap_or_default(),
                )
            })
            .collect();
        let row = Row::new(raw_idx + 1, cells);

        if !critical_columns.is_empty() && critical_columns.iter().all(|col| row.get(col).is_none())
        {
            continue;
        }
        if has_status_column
            && !matches!(
                row.get_str("Trafficking Status").to_lowercase().as_str(),
                "new" | "update"
            )
        {
            continue;
        }
        rows.push(row);
    }
    Ok(rows)
}

/// Campaign metadata read from the first two rows of the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetMetadata {
    /// CM360 user profile ID.
    pub profile_id: String,
    /// CM360 advertiser ID.
    pub advertiser_id: String,
    /// CM360 campaign ID.
    pub campaign_id: String,
    /// Optional campaign name.
    pub campaign_name: Option<String>,
}

/// Port of `clean_id`: strips, treats `""`/`none`/`nan`/`null` as missing and
/// removes a trailing `".0"`.
fn clean_id(val: Option<&str>) -> Option<String> {
    let val = val?;
    if is_na(val) {
        return None;
    }
    let val_str = val.trim();
    if matches!(
        val_str.to_lowercase().as_str(),
        "" | "none" | "nan" | "null"
    ) {
        return None;
    }
    Some(val_str.strip_suffix(".0").unwrap_or(val_str).to_string())
}

/// Port of `prepare_data`: scans ONLY the first header/value row pair (the
/// Python loop breaks after `row_idx == 0`). For each cell in row 0 whose
/// trimmed lowercase value contains `"profile id"` / `"advertiser id"` /
/// `"campaign id"` / `"campaign name"`, the same column of row 1 is taken
/// (IDs are cleaned with `clean_id`). Then returns [`clean_sheet`]`(raw)`
/// together with the metadata.
///
/// # Errors
///
/// `"Missing required campaign metadata field: Profile ID in the first two
/// rows."` (likewise for `Advertiser ID` / `Campaign ID`), or any
/// [`clean_sheet`] error.
pub fn prepare_data(raw: &RawSheet) -> anyhow::Result<(Vec<Row>, SheetMetadata)> {
    let mut profile_id: Option<String> = None;
    let mut advertiser_id: Option<String> = None;
    let mut campaign_id: Option<String> = None;
    let mut campaign_name: Option<String> = None;

    if let [header_row, values_row, ..] = raw.as_slice() {
        for (col_idx, col_val) in header_row.iter().enumerate() {
            let col_str = col_val.trim().to_lowercase();
            let value = values_row.get(col_idx).map(String::as_str);
            if col_str.contains("profile id") {
                profile_id = clean_id(value);
            } else if col_str.contains("advertiser id") {
                advertiser_id = clean_id(value);
            } else if col_str.contains("campaign id") {
                campaign_id = clean_id(value);
            } else if col_str.contains("campaign name")
                && let Some(val) = value.filter(|v| !is_na(v))
            {
                let val_str = val.trim();
                if !matches!(
                    val_str.to_lowercase().as_str(),
                    "" | "none" | "nan" | "null"
                ) {
                    campaign_name = Some(val_str.to_string());
                }
            }
        }
    }

    let Some(profile_id) = profile_id else {
        bail!("Missing required campaign metadata field: Profile ID in the first two rows.");
    };
    let Some(advertiser_id) = advertiser_id else {
        bail!("Missing required campaign metadata field: Advertiser ID in the first two rows.");
    };
    let Some(campaign_id) = campaign_id else {
        bail!("Missing required campaign metadata field: Campaign ID in the first two rows.");
    };

    let rows = clean_sheet(raw)?;
    Ok((
        rows,
        SheetMetadata {
            profile_id,
            advertiser_id,
            campaign_id,
            campaign_name,
        },
    ))
}

/// Pure part of `_update_trafficking_sheet_status`.
///
/// Locates the header (same search as [`find_header_row`]), finds the
/// `"trafficking status"`, `"ad name"` and `"placement name"` columns
/// (case-insensitive trimmed compare) and, for each data row whose ad name is
/// in `successful_ads` or whose placement name is in `successful_placements`,
/// sets the status cell to `"Trafficked"` (padding the row with empty cells
/// when it is shorter than the status column index).
///
/// Returns `None` (after logging a warning) when the header or any of the
/// three columns is missing, otherwise the updated sheet and the number of
/// updated rows. Callers should skip the call entirely when both name sets
/// are empty (the Python code returns early before loading the artifact).
pub fn apply_trafficked_status(
    raw: &RawSheet,
    successful_ads: &HashSet<String>,
    successful_placements: &HashSet<String>,
) -> Option<(RawSheet, usize)> {
    let Some(header_idx) = find_header_row(raw) else {
        warn!(
            "⚠️ [traffic_campaigns_in_cm360_tool] - Could not locate header row in the \
             trafficking sheet to update status."
        );
        return None;
    };

    let mut status_col: Option<usize> = None;
    let mut ad_name_col: Option<usize> = None;
    let mut placement_name_col: Option<usize> = None;
    for (col_idx, col_name) in raw[header_idx].iter().enumerate() {
        match col_name.trim().to_lowercase().as_str() {
            "trafficking status" => status_col = Some(col_idx),
            "ad name" => ad_name_col = Some(col_idx),
            "placement name" => placement_name_col = Some(col_idx),
            _ => {}
        }
    }
    let (Some(status_col), Some(ad_name_col), Some(placement_name_col)) =
        (status_col, ad_name_col, placement_name_col)
    else {
        warn!(
            "⚠️ [traffic_campaigns_in_cm360_tool] - Required columns ('Trafficking Status', \
             'Placement Name', or 'Ad Name') not found in the trafficking sheet."
        );
        return None;
    };

    let mut updated = raw.clone();
    let mut updated_count = 0usize;
    for row in updated.iter_mut().skip(header_idx + 1) {
        let cell_at = |idx: usize| {
            row.get(idx)
                .filter(|cell| !is_na(cell))
                .map(|cell| cell.trim().to_string())
        };
        let (Some(ad_name), Some(placement_name)) =
            (cell_at(ad_name_col), cell_at(placement_name_col))
        else {
            continue;
        };
        if ad_name.is_empty() || placement_name.is_empty() {
            continue;
        }
        if successful_ads.contains(&ad_name) || successful_placements.contains(&placement_name) {
            if row.len() <= status_col {
                row.resize(status_col + 1, String::new());
            }
            row[status_col] = "Trafficked".to_string();
            updated_count += 1;
        }
    }
    Some((updated, updated_count))
}

/// Shared test fixtures for the `cm360_trafficking` test modules.
#[cfg(test)]
pub(crate) mod test_support {
    use indexmap::IndexMap;

    use super::Row;

    /// Builds a [`Row`] (row number 1) from `(column, raw value)` pairs.
    pub(crate) fn row(cells: &[(&str, &str)]) -> Row {
        row_numbered(1, cells)
    }

    /// Builds a [`Row`] with an explicit row number from `(column, value)`
    /// pairs.
    pub(crate) fn row_numbered(row_number: usize, cells: &[(&str, &str)]) -> Row {
        Row::new(
            row_number,
            cells
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<IndexMap<String, String>>(),
        )
    }

    /// Port of the Python `make_test_csv` helper: a trafficking sheet CSV
    /// with two metadata rows, two filler rows, the header row and one data
    /// row (row number 6). `overrides` replace the defaults by key.
    pub(crate) fn make_test_csv(overrides: &[(&str, &str)]) -> String {
        let mut d: IndexMap<&str, &str> = [
            ("profile_id", "7023449"),
            ("advertiser_id", "13641571"),
            ("campaign_id", "30535365"),
            ("campaign_name", "Google Ad Spaces Testing - June 2026"),
            ("status", "New"),
            ("site_id", "4802860"),
            ("site", ""),
            ("placement_id", ""),
            ("placement_name", "Test~Placement~1 Test"),
            ("placement_size", "300x250"),
            ("compatibility", "DISPLAY"),
            ("payment_source", "PLACEMENT_AGENCY_PAID"),
            ("pricing_type", "PRICING_TYPE_CPM"),
            ("pricing_start", "8/29/3000"),
            ("pricing_end", "9/25/3000"),
            (
                "tag_formats",
                "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT",
            ),
            ("ad_id", ""),
            ("ad_name", "Test Ad C"),
            ("ad_start", "8/29/3000"),
            ("ad_end", "9/25/3000"),
            ("ad_type", "AD_SERVING_STANDARD_AD"),
            ("priority", "AD_PRIORITY_01"),
            ("impression_ratio", "1"),
            ("creative_id", "999123"),
            ("creative_name", "TEST_ACG~300x250"),
            ("click_through_url", "https://click.test.com"),
            ("ad_dynamic_click_tracker", "TRUE"),
            ("base_url", "https://www.test.com"),
            ("final_url", "https://www.test.com?utm_medium=test"),
            ("placement_status", "PLACEMENT_STATUS_ACTIVE"),
        ]
        .into_iter()
        .collect();
        for (key, value) in overrides {
            d.insert(key, value);
        }

        let r1 = format!(
            "Digitas-DFA 609,Profile ID,Advertiser ID,Campaign ID,Campaign Name{}\n",
            ",".repeat(41)
        );
        let r2 = format!(
            ",{},{},{},{}{}\n",
            d["profile_id"],
            d["advertiser_id"],
            d["campaign_id"],
            d["campaign_name"],
            ",".repeat(41)
        );
        let r3 = format!(
            "{}Placement Type Not in API - renamed to Compatibility{}\n",
            ",".repeat(9),
            ",".repeat(36)
        );
        let r4 = format!("{}\n", ",".repeat(45));
        let r5 = "Campaign ID,Campaign Name,Trafficking Status,Site ID,Site,Channel,\
                  Placement ID,Placement Name,Placement Size,Compatibility,\
                  Placement Status,Placement Payment Source,Pricing Schedule Type,\
                  Pricing Schedule Start Date,Pricing Schedule End Date,\
                  Placement Tag Formats,Ad Server,Ad ID,Ad Name,Ad Start Date,\
                  Ad End Date,Ad Type,Delivery Schedule Priority,\
                  Delivery Schedule Impression Ratio,Ad Click Through Url,\
                  Ad Dynamic Click Tracker,Creative ID,Creative Name,\
                  Creative Start Date,Creative End Date,Creative Rotation,\
                  Creative Dimensions,Creative Type,\
                  Creative asset file link,Base URL,Final Trafficking URL,\
                  Brand Safety/Verification,Brand Safety/Verification Measurement Type,\
                  Research Partner,Notes,Campaign Funding,Campaign Quarter,\
                  Creative Detail,Fiscal,Language,Salesforce ID\n";
        let r6 = format!(
            "{},{},{},{},{},display,{},{},{},{},{},{},{},{},{},\"{}\",DCM,{},{},{},{},{},{},{},{},{},{},{},\
             6/10/2026,7/10/2026,100%,300x250,HTML5_BANNER,,{},{},None,None,CINT,,,,,,,\n",
            d["campaign_id"],
            d["campaign_name"],
            d["status"],
            d["site_id"],
            d["site"],
            d["placement_id"],
            d["placement_name"],
            d["placement_size"],
            d["compatibility"],
            d["placement_status"],
            d["payment_source"],
            d["pricing_type"],
            d["pricing_start"],
            d["pricing_end"],
            d["tag_formats"],
            d["ad_id"],
            d["ad_name"],
            d["ad_start"],
            d["ad_end"],
            d["ad_type"],
            d["priority"],
            d["impression_ratio"],
            d["click_through_url"],
            d["ad_dynamic_click_tracker"],
            d["creative_id"],
            d["creative_name"],
            d["base_url"],
            d["final_url"],
        );
        format!("{r1}{r2}{r3}{r4}{r5}{r6}")
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::test_support::{make_test_csv, row};
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    #[test]
    fn is_na_matches_pandas_defaults() {
        for na in [
            "", " ", "#N/A", "#N/A N/A", "#NA", "-1.#IND", "-1.#QNAN", "-NaN", "-nan", "1.#IND",
            "1.#QNAN", "<NA>", "N/A", "NA", "NULL", "NaN", "None", "n/a", "nan", "null", "  NA  ",
        ] {
            assert!(is_na(na), "{na:?} should be NA");
        }
        for not_na in ["0", "none", "Null", "x", "Na", "NONE"] {
            assert!(!is_na(not_na), "{not_na:?} should not be NA");
        }
    }

    #[test]
    fn is_empty_semantics() {
        assert!(is_empty(None));
        assert!(is_empty(Some("")));
        assert!(is_empty(Some("   ")));
        assert!(is_empty(Some("None")));
        assert!(is_empty(Some("NONE")));
        assert!(is_empty(Some(" nan ")));
        assert!(is_empty(Some("NA")));
        assert!(is_empty(Some("null")));
        assert!(!is_empty(Some("0")));
        assert!(!is_empty(Some("value")));
    }

    #[test]
    fn row_accessors() {
        let r = row(&[("A", " x "), ("B", "NA"), ("C", "")]);
        assert_eq!(r.row_number, 1);
        assert_eq!(r.get("A"), Some(" x "));
        assert_eq!(r.get_str("A"), "x");
        assert_eq!(r.get("B"), None);
        assert_eq!(r.get_str("B"), "");
        assert_eq!(r.get("C"), None);
        assert_eq!(r.get("Missing"), None);
        assert!(r.has_column("B"));
        assert!(!r.has_column("Missing"));
        assert_eq!(r.columns().collect::<Vec<_>>(), vec!["A", "B", "C"]);
    }

    #[test]
    fn parse_csv_handles_ragged_rows_quotes_blank_lines_and_bom() {
        let content = "\u{feff}a,b,c\n\n1,\"x,y\"\n,,\n\"q\"\"uote\",2,3,4\n";
        let raw = parse_csv(content).unwrap();
        assert_eq!(
            raw,
            vec![
                vec!["a", "b", "c"],
                vec!["1", "x,y"],
                vec!["", "", ""],
                vec!["q\"uote", "2", "3", "4"],
            ]
            .into_iter()
            .map(|r| r.into_iter().map(str::to_string).collect::<Vec<_>>())
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn to_csv_round_trips_with_minimal_quoting() {
        let raw: RawSheet = vec![
            vec!["a".into(), "b,c".into(), "d\"e".into()],
            vec!["".into(), "None".into()],
        ];
        let text = to_csv(&raw).unwrap();
        assert_eq!(text, "a,\"b,c\",\"d\"\"e\"\n,None\n");
        assert_eq!(parse_csv(&text).unwrap(), raw);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(
            parse_size(Some("300x250")),
            Size {
                width: 300,
                height: 250
            }
        );
        assert_eq!(
            parse_size(Some(" 728 x 90 ")),
            Size {
                width: 728,
                height: 90
            }
        );
        assert_eq!(
            parse_size(Some("300X250")),
            Size {
                width: 300,
                height: 250
            }
        );
        let zero = Size {
            width: 0,
            height: 0,
        };
        assert_eq!(parse_size(None), zero);
        assert_eq!(parse_size(Some("invalid")), zero);
        assert_eq!(parse_size(Some("axb")), zero);
        assert_eq!(parse_size(Some("1x2x3")), zero);
        assert_eq!(parse_size(Some("")), zero);
        assert_eq!(
            serde_json::to_value(parse_size(Some("1x2"))).unwrap(),
            serde_json::json!({"width": 1, "height": 2})
        );
    }

    #[test]
    fn parse_datetime_accepts_many_formats() {
        let midnight = dt(2026, 6, 15, 0, 0, 0);
        for input in [
            "2026-06-15",
            " 2026-06-15 ",
            "6/15/2026",
            "06/15/2026",
            "6/15/26",
            "2026/06/15",
            "15-Jun-2026",
            "Jun 15, 2026",
            "June 15, 2026",
            "15 June 2026",
            "20260615",
            "2026-06-15T00:00:00Z",
            "2026-06-15T00:00:00",
            "2026-06-15 00:00:00",
        ] {
            assert_eq!(parse_datetime(input), Some(midnight), "input {input:?}");
        }
        let half_past = dt(2026, 6, 15, 12, 30, 0);
        for input in [
            "2026-06-15T12:30:00Z",
            "2026-06-15T12:30:00.000Z",
            "2026-06-15T12:30:00+05:00",
            "2026-06-15T12:30:00.123456-07:00",
            "2026-06-15T12:30:00+0500",
            "2026-06-15 12:30:00Z",
            "2026-06-15T12:30",
            "2026-06-15 12:30",
            "6/15/2026 12:30",
            "6/15/2026 12:30:00",
            "6/15/2026 12:30 PM",
        ] {
            let parsed = parse_datetime(input).unwrap_or_else(|| panic!("input {input:?}"));
            assert_eq!(
                parsed.format("%Y-%m-%dT%H:%M:%S").to_string(),
                half_past.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "input {input:?}"
            );
        }
        assert_eq!(parse_datetime("8/29/3000"), Some(dt(3000, 8, 29, 0, 0, 0)));
        for invalid in [
            "",
            "   ",
            "not-a-date",
            "invalid-date",
            "not-a-date extra",
            "45000",
        ] {
            assert_eq!(parse_datetime(invalid), None, "input {invalid:?}");
        }
    }

    #[test]
    fn test_format_date() {
        assert_eq!(
            format_date(Some("2026-06-15")).as_deref(),
            Some("2026-06-15")
        );
        assert_eq!(
            format_date(Some("6/15/2026")).as_deref(),
            Some("2026-06-15")
        );
        assert_eq!(
            format_date(Some("8/29/3000")).as_deref(),
            Some("3000-08-29")
        );
        assert_eq!(
            format_date(Some("2026-06-15T12:30:00Z")).as_deref(),
            Some("2026-06-15")
        );
        assert_eq!(format_date(None), None);
        assert_eq!(
            format_date(Some("not-a-date extra")).as_deref(),
            Some("not-a-date")
        );
        assert_eq!(format_date(Some("   ")), None);
    }

    #[test]
    fn test_format_date_time() {
        assert_eq!(
            format_date_time(Some("2026-06-15")).as_deref(),
            Some("2026-06-15T00:00:00Z")
        );
        assert_eq!(
            format_date_time(Some("8/29/3000")).as_deref(),
            Some("3000-08-29T00:00:00Z")
        );
        assert_eq!(
            format_date_time(Some("2026-06-15T12:30:00+05:00")).as_deref(),
            Some("2026-06-15T12:30:00Z")
        );
        assert_eq!(format_date_time(None), None);
        assert_eq!(
            format_date_time(Some("not-a-date")).as_deref(),
            Some("not-a-date")
        );
        assert_eq!(format_date_time(Some("")).as_deref(), Some(""));
    }

    #[test]
    fn test_normalize_iso_datetime() {
        assert_eq!(
            normalize_iso_datetime(Some("2026-06-15T12:30:00Z")).as_deref(),
            Some("2026-06-15T12:30:00Z")
        );
        assert_eq!(
            normalize_iso_datetime(Some(" 2026-06-15 ")).as_deref(),
            Some("2026-06-15T00:00:00Z")
        );
        assert_eq!(normalize_iso_datetime(None), None);
        assert_eq!(normalize_iso_datetime(Some("null")), None);
        assert_eq!(normalize_iso_datetime(Some("NONE")), None);
        assert_eq!(normalize_iso_datetime(Some("nan")), None);
        assert_eq!(normalize_iso_datetime(Some("  ")), None);
        assert_eq!(
            normalize_iso_datetime(Some(" invalid-date ")).as_deref(),
            Some("invalid-date")
        );
    }

    #[test]
    fn test_normalize_placement_status() {
        assert_eq!(normalize_placement_status(None), "");
        assert_eq!(normalize_placement_status(Some("")), "");
        assert_eq!(
            normalize_placement_status(Some(" PLACEMENT_STATUS_ACTIVE ")),
            "PLACEMENT_STATUS_ACTIVE"
        );
    }

    #[test]
    fn test_parse_weight() {
        assert_eq!(parse_weight(None), 100);
        assert_eq!(parse_weight(Some("100%")), 100);
        assert_eq!(parse_weight(Some(" 50% ")), 50);
        assert_eq!(parse_weight(Some("0.5")), 50);
        assert_eq!(parse_weight(Some("1")), 100);
        assert_eq!(parse_weight(Some("1.0")), 100);
        assert_eq!(parse_weight(Some("75")), 75);
        assert_eq!(parse_weight(Some("75.9")), 75);
        assert_eq!(parse_weight(Some("0")), 0);
        assert_eq!(parse_weight(Some("abc")), 100);
        assert_eq!(parse_weight(Some("")), 100);
        assert_eq!(parse_weight(Some("inf")), 100);
        assert_eq!(parse_weight(Some("nan")), 100);
    }

    #[test]
    fn test_extract_site_identifier() {
        assert_eq!(
            extract_site_identifier(&row(&[("Site", " SiteA ")])),
            "SiteA"
        );
        assert_eq!(extract_site_identifier(&row(&[("Site", "NA")])), "");
        assert_eq!(extract_site_identifier(&row(&[("Other", "x")])), "");
    }

    fn sheet(rows: &[&[&str]]) -> RawSheet {
        rows.iter()
            .map(|r| r.iter().map(|c| (*c).to_string()).collect())
            .collect()
    }

    #[test]
    fn find_header_row_two_pass_search() {
        // First pass skips the metadata rows.
        let raw = sheet(&[
            &["Ad Name", "x"],
            &["meta", "y"],
            &["junk", ""],
            &[" Placement Name ", "Ad Name"],
            &["P1", "A1"],
        ]);
        assert_eq!(find_header_row(&raw), Some(3));
        // Second pass falls back to any row.
        let raw = sheet(&[&["Trafficking Status", "x"], &["New", "y"]]);
        assert_eq!(find_header_row(&raw), Some(0));
        let raw = sheet(&[&["a", "b"], &["c", "d"], &["e", "f"]]);
        assert_eq!(find_header_row(&raw), None);
        assert_eq!(find_header_row(&Vec::new()), None);
    }

    #[test]
    fn clean_sheet_errors_without_header() {
        let raw = sheet(&[&["a", "b"], &["c", "d"]]);
        let err = clean_sheet(&raw).unwrap_err();
        assert_eq!(err.to_string(), "Could not find table header in the sheet.");
    }

    #[test]
    fn clean_sheet_filters_rows_and_assigns_row_numbers() {
        let raw = sheet(&[
            &["Profile ID", "Advertiser ID"],
            &["1", "2"],
            &[
                "Trafficking Status",
                " Placement Name ",
                "Ad Name",
                "",
                "NA",
                "Extra",
            ],
            &["New", "P1", "A1", "ignored", "ignored", "e1"],
            &["", "", "", "", "", ""],
            &["Update", "NA", "", "", "", "no identifiers"],
            &["Trafficked", "P2", "A2"],
            &["update", " P3 ", "A3"],
            &["new", "", "A4", "x"],
        ]);
        let rows = clean_sheet(&raw).unwrap();
        let summary: Vec<(usize, String, String)> = rows
            .iter()
            .map(|r| {
                (
                    r.row_number,
                    r.get_str("Placement Name"),
                    r.get_str("Ad Name"),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (4, "P1".to_string(), "A1".to_string()),
                (8, "P3".to_string(), "A3".to_string()),
                (9, String::new(), "A4".to_string()),
            ]
        );
        // Empty / NA header cells produce no column; names are trimmed.
        assert_eq!(
            rows[0].columns().collect::<Vec<_>>(),
            vec!["Trafficking Status", "Placement Name", "Ad Name", "Extra"]
        );
        assert_eq!(rows[0].get("Extra"), Some("e1"));
        // Ragged rows: missing cells behave like NA.
        assert_eq!(rows[1].get("Extra"), None);
    }

    #[test]
    fn clean_sheet_without_status_column_keeps_all_identified_rows() {
        let raw = sheet(&[
            &["Placement Name", "Creative Name"],
            &["P1", ""],
            &["", "C1"],
            &["", ""],
        ]);
        let rows = clean_sheet(&raw).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.row_number).collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn prepare_data_extracts_metadata_from_test_csv() {
        let raw = parse_csv(&make_test_csv(&[])).unwrap();
        assert_eq!(raw.len(), 6);
        let (rows, meta) = prepare_data(&raw).unwrap();
        assert_eq!(
            meta,
            SheetMetadata {
                profile_id: "7023449".to_string(),
                advertiser_id: "13641571".to_string(),
                campaign_id: "30535365".to_string(),
                campaign_name: Some("Google Ad Spaces Testing - June 2026".to_string()),
            }
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_number, 6);
        assert_eq!(rows[0].get_str("Placement Name"), "Test~Placement~1 Test");
        assert_eq!(rows[0].get_str("Ad Name"), "Test Ad C");
        assert_eq!(
            rows[0].get_str("Placement Tag Formats"),
            "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT"
        );
        assert_eq!(rows[0].get("Site"), None);
        assert_eq!(rows[0].get("Brand Safety/Verification"), None);
    }

    #[test]
    fn prepare_data_campaign_name_is_optional_and_ids_are_cleaned() {
        let raw = parse_csv(&make_test_csv(&[
            ("campaign_name", ""),
            ("profile_id", "7023449.0"),
        ]))
        .unwrap();
        let (_, meta) = prepare_data(&raw).unwrap();
        assert_eq!(meta.campaign_name, None);
        assert_eq!(meta.profile_id, "7023449");

        let raw = parse_csv(&make_test_csv(&[("campaign_name", "null")])).unwrap();
        assert_eq!(prepare_data(&raw).unwrap().1.campaign_name, None);
    }

    #[test]
    fn prepare_data_errors_on_missing_required_ids() {
        for (key, field) in [
            ("profile_id", "Profile ID"),
            ("advertiser_id", "Advertiser ID"),
            ("campaign_id", "Campaign ID"),
        ] {
            let raw = parse_csv(&make_test_csv(&[(key, "")])).unwrap();
            let err = prepare_data(&raw).unwrap_err().to_string();
            assert_eq!(
                err,
                format!("Missing required campaign metadata field: {field} in the first two rows.")
            );
        }
        // "nan" / "None" style values count as missing too.
        let raw = parse_csv(&make_test_csv(&[("campaign_id", "None")])).unwrap();
        assert!(
            prepare_data(&raw)
                .unwrap_err()
                .to_string()
                .contains("Campaign ID")
        );
        // Only the first row pair is scanned.
        let raw = sheet(&[
            &["x"],
            &["Profile ID", "Advertiser ID", "Campaign ID"],
            &["1", "2", "3"],
        ]);
        assert!(
            prepare_data(&raw)
                .unwrap_err()
                .to_string()
                .contains("Profile ID")
        );
        assert!(
            prepare_data(&Vec::new())
                .unwrap_err()
                .to_string()
                .contains("Profile ID")
        );
    }

    #[test]
    fn apply_trafficked_status_updates_matching_rows() {
        let raw = sheet(&[
            &["Profile ID", "Advertiser ID", "Campaign ID"],
            &["1", "2", "3"],
            &["trafficking status", "PLACEMENT NAME", " Ad Name "],
            &["New", "P1", "A1"],
            &["New", "P2", "A2"],
            &["New", "P3", "A3"],
            &["New", "", "A1"],
            &["New", "NA", "A1"],
            &["New", "P1"],
            &["New", "P1", "A9", "extra"],
        ]);
        let ads: HashSet<String> = ["A1".to_string()].into_iter().collect();
        let placements: HashSet<String> = ["P3".to_string()].into_iter().collect();
        let (updated, count) = apply_trafficked_status(&raw, &ads, &placements).unwrap();
        assert_eq!(count, 2);
        assert_eq!(updated[3][0], "Trafficked");
        assert_eq!(updated[4][0], "New");
        assert_eq!(updated[5][0], "Trafficked");
        assert_eq!(updated[6][0], "New");
        assert_eq!(updated[7][0], "New");
        assert_eq!(updated[8][0], "New");
        assert_eq!(updated[9][0], "New");
        // Original is untouched.
        assert_eq!(raw[3][0], "New");
    }

    #[test]
    fn apply_trafficked_status_pads_short_rows() {
        let raw = sheet(&[
            &["Ad Name", "Placement Name", "Trafficking Status"],
            &["A1", "P1"],
        ]);
        let ads: HashSet<String> = ["A1".to_string()].into_iter().collect();
        let (updated, count) = apply_trafficked_status(&raw, &ads, &HashSet::new()).unwrap();
        assert_eq!(count, 1);
        assert_eq!(updated[1], vec!["A1", "P1", "Trafficked"]);
    }

    #[test]
    fn apply_trafficked_status_returns_none_when_columns_missing() {
        let ads: HashSet<String> = ["A1".to_string()].into_iter().collect();
        let no_header = sheet(&[&["a", "b"], &["c", "d"]]);
        assert_eq!(
            apply_trafficked_status(&no_header, &ads, &HashSet::new()),
            None
        );
        let no_status = sheet(&[&["Ad Name", "Placement Name"], &["A1", "P1"]]);
        assert_eq!(
            apply_trafficked_status(&no_status, &ads, &HashSet::new()),
            None
        );
        let no_placement = sheet(&[&["Ad Name", "Trafficking Status"], &["A1", "New"]]);
        assert_eq!(
            apply_trafficked_status(&no_placement, &ads, &HashSet::new()),
            None
        );
    }

    #[test]
    fn apply_trafficked_status_round_trips_through_csv() {
        let raw = parse_csv(&make_test_csv(&[])).unwrap();
        let ads: HashSet<String> = ["Test Ad C".to_string()].into_iter().collect();
        let (updated, count) = apply_trafficked_status(&raw, &ads, &HashSet::new()).unwrap();
        assert_eq!(count, 1);
        let text = to_csv(&updated).unwrap();
        let reparsed = parse_csv(&text).unwrap();
        assert_eq!(reparsed, updated);
        assert_eq!(reparsed[5][2], "Trafficked");
        assert!(text.contains("\"PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT\""));
    }
}
