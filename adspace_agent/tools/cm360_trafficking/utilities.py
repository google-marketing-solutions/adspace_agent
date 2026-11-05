# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Utility functions for Campaign Manager 360 trafficking operations."""

import asyncio
import datetime
import io
import logging
import operator
import pathlib
import re
import urllib.parse
from zoneinfo import ZoneInfo
from zoneinfo import ZoneInfoNotFoundError

from google.adk.tools.tool_context import ToolContext
from google.cloud import storage
import pandas as pd

logger = logging.getLogger(__name__)


def parse_size(val: object) -> dict[str, int]:
    """Parses a size string like '300x250' into a width/height dictionary.

    Args:
        val: The raw value to parse.

    Returns:
        A dictionary with 'width' and 'height' keys as integers.
    """
    if pd.isna(val):
        return {"width": 0, "height": 0}
    s = str(val).lower().replace(" ", "").split("x")
    if len(s) == 2:  # ruff: ignore[magic-value-comparison]
        try:
            return {"width": int(s[0]), "height": int(s[1])}
        except ValueError:
            pass
    return {"width": 0, "height": 0}


GLOBAL_METADATA_ROWS_COUNT = 2


def format_date(val: object) -> str | None:
    """Formats date values into a standard YYYY-MM-DD string.

    Args:
        val: The raw date value to format.

    Returns:
        The formatted date string in YYYY-MM-DD format, or None if missing.
    """
    if pd.isna(val):
        return None
    try:
        parsed = pd.to_datetime(str(val))
        return parsed.strftime("%Y-%m-%d")
    except (ValueError, TypeError, KeyError):
        return str(val).split()[0]


def format_date_time(
    val: object,
    time_zone: str,
    *,
    is_end_date: bool = False,
) -> str | None:
    """Formats datetime values into an ISO-8601 UTC date-time string.

    Args:
        val: The raw datetime value to format.
        time_zone: IANA time zone string (e.g. 'America/New_York').
            Localizes the date to this time zone before converting to UTC.
        is_end_date: Whether this represents an end date. If True and no time
            is explicitly given, sets time to 23:59:59. Otherwise sets to
            00:00:00.

    Returns:
        The formatted date-time string in ISO-8601 format, or None if missing.

    Raises:
        ValueError: If time_zone is empty or not a valid IANA time zone.
    """
    if pd.isna(val) or val is None:
        return None
    val_str = str(val).strip()
    if not val_str or val_str.lower() in {"none", "nan", "null"}:
        return None

    if not time_zone or not str(time_zone).strip():
        msg = "Time zone must be provided and cannot be empty."
        raise ValueError(msg)

    tz = ZoneInfo(str(time_zone).strip())

    try:
        parsed = pd.to_datetime(val_str)
    except (ValueError, TypeError, KeyError):
        return val_str

    # If already formatted with 'T' and timezone indicator
    if "T" in val_str and (
        "Z" in val_str or "+" in val_str or "-" in val_str[10:]
    ):
        if parsed.tzinfo is None:
            parsed = parsed.tz_localize("UTC")
        else:
            parsed = parsed.tz_convert("UTC")
        return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")

    time_part = (
        datetime.time(23, 59, 59) if is_end_date else datetime.time(0, 0, 0)
    )
    local_dt = datetime.datetime.combine(parsed.date(), time_part, tzinfo=tz)
    utc_dt = local_dt.astimezone(datetime.UTC)
    return utc_dt.strftime("%Y-%m-%dT%H:%M:%SZ")


def normalize_iso_datetime(val: object) -> str | None:
    """Normalizes datetime values into a standard UTC ISO-8601 string.

    Args:
        val: Raw datetime string, timestamp, or None.

    Returns:
        Formatted datetime string 'YYYY-MM-DDTHH:MM:SSZ' or None.
    """
    if pd.isna(val) or val is None:
        return None
    val_str = str(val).strip()
    if not val_str or val_str.lower() in {"none", "nan", "null"}:
        return None
    try:
        parsed = pd.to_datetime(val_str)
        return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")
    except (ValueError, TypeError, KeyError):
        return val_str


def normalize_placement_status(val: object) -> str:
    """Returns placement activeStatus enum string directly from sheet.

    Args:
        val: Raw status string or object.

    Returns:
        Stripped placement activeStatus enum string, or empty string.
    """
    if pd.isna(val) or not val:
        return ""
    return str(val).strip()


def parse_weight(val: object) -> int:
    """Parses creative rotation weights (multiplies decimals by 100).

    Args:
        val: The raw rotation value to parse.

    Returns:
        An integer representing the rotation weight percentage.
    """
    if pd.isna(val):
        return 100
    try:
        s_val = str(val).strip().replace("%", "")
        f_val = float(s_val)
        if f_val <= 1.0:
            return int(f_val * 100)
        return int(f_val)
    except ValueError:
        return 100


def clean_dataframe(  # ruff: ignore[complex-structure]
    df_raw: pd.DataFrame,
) -> pd.DataFrame:
    """Finds the header dynamically and cleans the campaign trafficking data.

    Locates the table header row, cleans column names, removes empty rows,
    drops non-data rows lacking critical identifiers, and filters for rows with
    actionable trafficking status ('new' or 'update').

    Args:
        df_raw: Raw pandas DataFrame loaded from file or memory.

    Returns:
        A cleaned pandas DataFrame ready for validation and entity extraction.

    Raises:
        ValueError: If the table header row cannot be located in the sheet.
    """
    header_row_idx: int | None = None
    for idx, row in df_raw.iterrows():
        if not isinstance(idx, int) or idx < GLOBAL_METADATA_ROWS_COUNT:
            continue
        row_values = [str(v).strip() for v in row.to_numpy() if not pd.isna(v)]
        if (
            "Trafficking Status" in row_values
            or "Placement Name" in row_values
            or "Ad Name" in row_values
        ):
            header_row_idx = idx
            break

    if header_row_idx is None:
        for idx, row in df_raw.iterrows():
            if not isinstance(idx, int):
                continue
            row_values = [
                str(v).strip() for v in row.to_numpy() if not pd.isna(v)
            ]
            if (
                "Trafficking Status" in row_values
                or "Placement Name" in row_values
                or "Ad Name" in row_values
            ):
                header_row_idx = idx
                break

    if header_row_idx is None:
        msg = "Could not find table header in the sheet."
        raise ValueError(msg)

    # Re-align df with correct header row
    df = df_raw.iloc[header_row_idx + 1 :].copy()
    df.columns = df_raw.iloc[header_row_idx]

    # Drop rows that are completely empty
    df = df.dropna(how="all")

    # Drop rows where all critical identifier fields are empty
    # (likely non-data rows)
    subset_cols = [
        c
        for c in ["Placement Name", "Ad Name", "Creative Name"]
        if c in df.columns
    ]
    if subset_cols:
        df = df.dropna(subset=subset_cols, how="all")

    df["Row Number"] = df.index + 1
    df = df.reset_index(drop=True)

    if "Trafficking Status" in df.columns:
        df = df[
            df["Trafficking Status"]
            .astype(str)
            .str.strip()
            .str.lower()
            .isin(["new", "update", "partially_failed", "failed"])
        ]

    return df


def extract_site_identifier(row: pd.Series) -> str:
    """Extracts the site identifier string from a DataFrame row.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.

    Returns:
        The extracted site identifier string, or empty string if not present.
    """
    if "Site" in row and not pd.isna(row["Site"]):
        return str(row["Site"]).strip()
    return ""


def _find_latest_artifact_from_events(
    session: object | None,
    matching_set: set[str],
) -> str | None:
    """Finds the most recently referenced artifact filename from session events.

    Args:
        session: Active session object if available.
        matching_set: Set of valid matching filenames.

    Returns:
        The matched artifact filename or None.
    """
    if not session:
        return None
    events = getattr(session, "events", []) or []
    for event in reversed(events):
        content = getattr(event, "content", None)
        parts = getattr(content, "parts", []) if content else []
        for part in parts:
            fn_call = getattr(part, "function_call", None)
            if fn_call and getattr(fn_call, "name", "") == "load_artifacts":
                args = getattr(fn_call, "args", {}) or {}
                for name in args.get("artifact_names", []):
                    if name in matching_set:
                        return name

            text = getattr(part, "text", "") or ""
            if "[Uploaded Artifact:" in text:
                matches = re.findall(r'\[Uploaded Artifact: "([^"]+)"\]', text)
                for name in matches:
                    if name in matching_set:
                        return name
    return None


def _extract_artifact_mtime(canonical_uri: str | None) -> float:
    """Extracts local file mtime from canonical URI if available.

    Args:
        canonical_uri: Canonical file URI string.

    Returns:
        File mtime float or 0.0.
    """
    if not canonical_uri:
        return 0.0
    parsed = urllib.parse.urlparse(canonical_uri)
    local_path = urllib.parse.unquote(parsed.path)
    p = pathlib.Path(local_path)
    if p.exists():
        return p.stat().st_mtime
    return 0.0


async def get_latest_csv_artifact(
    tool_context: ToolContext,
    matching_files: list[str],
) -> str:
    """Finds the most recently uploaded CSV artifact in the session.

    Determines the latest uploaded artifact using session upload events and
    artifact modification timestamps on disk, regardless of filename.

    Args:
        tool_context: Active tool context containing session artifacts.
        matching_files: List of CSV filenames available in session artifacts.

    Returns:
        The filename of the most recently uploaded CSV artifact.
    """
    if len(matching_files) == 1:
        return matching_files[0]

    matching_set = set(matching_files)

    # 1. Check session events in reverse order for recent upload references
    session = getattr(tool_context, "session", None)
    event_match = _find_latest_artifact_from_events(session, matching_set)
    if event_match:
        logger.info(
            "🎯 Found latest artifact from session event: %s", event_match
        )
        return event_match

    # 2. Check artifact creation / modification timestamps from artifact storage
    file_timestamps: list[tuple[float, str]] = []
    for filename in matching_files:
        mtime = 0.0
        try:
            ver = await tool_context.get_artifact_version(filename)
            if ver and ver.canonical_uri:
                mtime = await asyncio.to_thread(
                    _extract_artifact_mtime, ver.canonical_uri
                )
            elif ver and getattr(ver, "create_time", None):
                mtime = float(ver.create_time)
        except (OSError, ValueError, TypeError) as exc:
            logger.debug(
                "Could not determine timestamp for artifact '%s': %s",
                filename,
                exc,
            )
        file_timestamps.append((mtime, filename))

    # Sort by timestamp ascending; the highest timestamp (latest) is last
    file_timestamps.sort(key=operator.itemgetter(0))
    latest_file = file_timestamps[-1][1]
    logger.info(
        "🎯 Selected latest uploaded artifact by timestamp: %s", latest_file
    )
    return latest_file


async def load_raw_dataframe(
    tool_context: ToolContext,
    target_filename: str | None = None,
) -> tuple[pd.DataFrame, str]:
    """Lists, loads, and parses the trafficking CSV file from session artifacts.

    Args:
        tool_context: Active tool context containing session artifacts.
        target_filename: Optional specific CSV artifact filename to load.

    Returns:
        A tuple of (df_raw, target_filename) where df_raw is the loaded
        DataFrame.

    Raises:
        ValueError: If no CSV files are found or if the artifact cannot be
            loaded.
    """
    df_raw = None

    # 1. List all available artifacts in the session
    artifacts = await tool_context.list_artifacts()
    logger.info("📂 Discovered artifacts in session: %s", artifacts)

    # 2. Filter for CSV files (any filename)
    matching_files = [f for f in artifacts if f.lower().endswith(".csv")]

    if not matching_files:
        msg = (
            "No CSV trafficking files (.csv) found in session artifacts."
            " Please upload your trafficking CSV file."
        )
        raise ValueError(msg)

    # Select the latest uploaded matching CSV file
    if not target_filename or target_filename not in matching_files:
        target_filename = await get_latest_csv_artifact(
            tool_context, matching_files
        )

    logger.info("🎯 Selecting target campaign file: %s", target_filename)

    # 3. Load the selected artifact content
    part = await tool_context.load_artifact(target_filename)
    if not part:
        msg = f"Artifact '{target_filename}' could not be loaded."
        raise ValueError(msg)

    # 4. Extract raw CSV data
    if part.text:
        df_raw = pd.read_csv(io.StringIO(part.text), header=None)
    elif part.inline_data and part.inline_data.data:
        df_raw = pd.read_csv(io.BytesIO(part.inline_data.data), header=None)

    if df_raw is None:
        msg = f"Failed to extract data from artifact '{target_filename}'."
        raise ValueError(msg)

    return df_raw, target_filename


def prepare_data(  # ruff: ignore[complex-structure, too-many-branches, too-many-statements]
    df_raw: pd.DataFrame,
) -> tuple[pd.DataFrame, str, str, str, str | None]:
    """Extracts metadata from header rows and cleans the trafficking data.

    Args:
        df_raw: Raw DataFrame loaded from CSV.

    Returns:
        A tuple of (cleaned_df, profile_id, advertiser_id, campaign_id,
        campaign_name).

    Raises:
        ValueError: If required metadata fields (Profile ID, Advertiser ID,
            Campaign ID) are missing.
    """
    profile_id = None
    advertiser_id = None
    campaign_id = None
    campaign_name = None

    def clean_id(val: str | float | None) -> str | None:
        """Cleans and strips ID values from floats/strings.

        Args:
            val: Raw value to clean.

        Returns:
            Cleaned ID string, or None if empty.
        """
        if val is None or pd.isna(val):
            return None
        val_str = str(val).strip()
        if val_str.lower() in {"", "none", "nan", "null"}:
            return None
        return val_str.removesuffix(".0")

    time_zone = ""
    # Extract Profile ID, Advertiser ID, Campaign ID, Campaign Name,
    # and Time Zone from the first two rows (global config)
    for row_idx in range(len(df_raw) - 1):
        header_row = df_raw.iloc[row_idx]
        values_row = df_raw.iloc[row_idx + 1]
        for col_idx, col_val in enumerate(header_row):
            col_str = str(col_val).strip().lower()
            if "profile id" in col_str:
                profile_id = clean_id(values_row.iloc[col_idx])
            elif "advertiser id" in col_str:
                advertiser_id = clean_id(values_row.iloc[col_idx])
            elif "campaign id" in col_str:
                campaign_id = clean_id(values_row.iloc[col_idx])
            elif "campaign name" in col_str:
                val = values_row.iloc[col_idx]
                if val is not None and not pd.isna(val):
                    val_str = str(val).strip()
                    if val_str.lower() not in {"", "none", "nan", "null"}:
                        campaign_name = val_str
            elif "time zone" in col_str or "timezone" in col_str:
                val = values_row.iloc[col_idx]
                if val is not None and not pd.isna(val):
                    val_str = str(val).strip()
                    if val_str.lower() not in {"", "none", "nan", "null"}:
                        time_zone = val_str
        break

    if not profile_id:
        msg = (
            "Missing required campaign metadata field: Profile ID in the first"
            " two rows."
        )
        raise ValueError(msg)
    if not advertiser_id:
        msg = (
            "Missing required campaign metadata field: Advertiser ID in the"
            " first two rows."
        )
        raise ValueError(msg)
    if not campaign_id:
        msg = (
            "Missing required campaign metadata field: Campaign ID in the"
            " first two rows."
        )
        raise ValueError(msg)

    # Clean and parse data rows
    df = clean_dataframe(df_raw)
    if "Time Zone" in df.columns:
        valid_tz_series = df["Time Zone"].dropna()
        if not time_zone and not valid_tz_series.empty:
            time_zone = str(valid_tz_series.iloc[0]).strip()
        if time_zone:
            df["Time Zone"] = df["Time Zone"].fillna(time_zone)
    elif time_zone:
        df["Time Zone"] = time_zone
    else:
        df["Time Zone"] = ""

    if not time_zone:
        msg = (
            "Missing required campaign metadata field: Time Zone in the"
            " first two rows or as a column in the sheet."
        )
        raise ValueError(msg)

    try:
        ZoneInfo(time_zone)
    except (ZoneInfoNotFoundError, ValueError, KeyError) as exc:
        msg = (
            f"Invalid Time Zone '{time_zone}'. Must be a valid IANA time zone"
            " identifier (e.g. 'America/New_York', 'America/Chicago',"
            " 'America/Los_Angeles')."
        )
        raise ValueError(msg) from exc

    return df, profile_id, advertiser_id, campaign_id, campaign_name


def upload_to_gcs(
    bucket_name: str,
    object_path: str,
    content: str,
) -> str:
    """Uploads content string to a GCS bucket and returns the gs:// URL.

    Args:
        bucket_name: Name of the target Google Cloud Storage bucket.
        object_path: Destination object path within the bucket.
        content: String content (e.g., JSON) to upload.

    Returns:
        The full GCS URL string (e.g. 'gs://bucket/object_path').
    """
    logger.info(
        "Uploading payloads to GCS bucket=%s path=%s...",
        bucket_name,
        object_path,
    )
    client = storage.Client()
    bucket = client.bucket(bucket_name)
    blob = bucket.blob(object_path)
    blob.upload_from_string(content, content_type="application/json")
    gcs_url = f"gs://{bucket_name}/{object_path}"
    logger.info("Uploaded successfully to %s", gcs_url)
    return gcs_url


def download_from_gcs(
    bucket_name: str,
    object_path: str,
) -> str:
    """Downloads text content from a GCS bucket.

    Args:
        bucket_name: Name of the Google Cloud Storage bucket.
        object_path: Object path within the bucket to download.

    Returns:
        The downloaded text content as a string.
    """
    logger.info(
        "Downloading payloads from GCS bucket=%s path=%s...",
        bucket_name,
        object_path,
    )
    client = storage.Client()
    bucket = client.bucket(bucket_name)
    blob = bucket.blob(object_path)
    return blob.download_as_text()
