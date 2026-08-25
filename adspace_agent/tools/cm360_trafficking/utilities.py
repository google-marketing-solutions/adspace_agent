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

import io
import logging
from typing import Any

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


def format_date(val: object) -> str | None:
    """Formats datetime values into a YYYY-MM-DD string.

    Args:
        val: The raw date value to format.

    Returns:
        The formatted date string in YYYY-MM-DD format, or None if missing.
    """
    if pd.isna(val):
        return None
    try:
        parsed = pd.to_datetime(val)
        return parsed.strftime("%Y-%m-%d")
    except Exception:
        return str(val).split()[0]


def format_date_time(val: object) -> str | None:
    """Formats datetime values into an ISO-8601 date-time string.

    Args:
        val: The raw datetime value to format.

    Returns:
        The formatted date-time string in ISO-8601 format, or None if missing.
    """
    if pd.isna(val):
        return None
    try:
        parsed = pd.to_datetime(val)
        return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")
    except Exception:
        return str(val)


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


def clean_dataframe(df_raw: pd.DataFrame) -> pd.DataFrame:
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
    header_row_idx = None
    for idx, row in df_raw.iterrows():
        if idx < 2:  # Skip the top global metadata rows
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
            row_values = [str(v).strip() for v in row.to_numpy() if not pd.isna(v)]
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

    # Drop rows where all critical identifier fields are empty (likely non-data rows)
    subset_cols = [
        c for c in ["Placement Name", "Ad Name", "Creative Name"] if c in df.columns
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
            .isin(["new", "update"])
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


async def load_raw_dataframe(
    tool_context: ToolContext,
) -> tuple[pd.DataFrame, str]:
    """Lists, loads, and parses the trafficking CSV file from session artifacts.

    Args:
        tool_context: Active tool context containing session artifacts.

    Returns:
        A tuple of (df_raw, target_filename) where df_raw is the loaded DataFrame.

    Raises:
        ValueError: If no CSV files are found or if the artifact cannot be loaded.
    """
    df_raw = None

    # 1. List all available artifacts in the session
    artifacts = await tool_context.list_artifacts()
    logger.info("📂 Discovered artifacts in session: %s", artifacts)

    # 2. Filter for CSV files (any filename)
    matching_files = [f for f in artifacts if f.lower().endswith(".csv")]

    if not matching_files:
        msg = "No CSV trafficking files (.csv) found in session artifacts. Please upload your trafficking CSV file."
        raise ValueError(msg)

    # Select the latest uploaded matching CSV file
    target_filename = matching_files[-1]
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


def prepare_data(
    df_raw: pd.DataFrame,
) -> tuple[pd.DataFrame, str, str, str, str | None]:
    """Extracts metadata from global header rows and cleans the trafficking data.

    Args:
        df_raw: Raw DataFrame loaded from CSV.

    Returns:
        A tuple of (cleaned_df, profile_id, advertiser_id, campaign_id, campaign_name).

    Raises:
        ValueError: If required metadata fields (Profile ID, Advertiser ID, Campaign ID) are missing.
    """
    profile_id = None
    advertiser_id = None
    campaign_id = None
    campaign_name = None

    def clean_id(val: Any) -> str | None:
        """Cleans and strips ID values from floats/strings."""
        if val is None or pd.isna(val):
            return None
        val_str = str(val).strip()
        if val_str.lower() in {"", "none", "nan", "null"}:
            return None
        val_str = val_str.removesuffix(".0")
        return val_str

    # Extract Profile ID, Advertiser ID, Campaign ID, and Campaign Name from the first two rows (global config)
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
        break

    if not profile_id:
        msg = "Missing required campaign metadata field: Profile ID in the first two rows."
        raise ValueError(msg)
    if not advertiser_id:
        msg = "Missing required campaign metadata field: Advertiser ID in the first two rows."
        raise ValueError(msg)
    if not campaign_id:
        msg = "Missing required campaign metadata field: Campaign ID in the first two rows."
        raise ValueError(msg)

    # Clean and parse data rows
    df = clean_dataframe(df_raw)

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
