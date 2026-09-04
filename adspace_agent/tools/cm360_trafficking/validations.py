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
"""Validation rules for Campaign Manager 360 trafficking sheets."""

import datetime
from typing import Any

import pandas as pd

MAX_PLACEMENT_NAME_LENGTH = 512
MAX_AD_NAME_LENGTH = 256


def _is_empty(val: str | float | None) -> bool:
    """Checks if a value is empty, None, pd.isna, 'nan', or 'none'.

    Args:
        val: The value to inspect.

    Returns:
        True if the value is considered empty, False otherwise.
    """
    if val is None or pd.isna(val):
        return True
    val_str = str(val).strip().lower()
    return val_str in {"", "none", "nan"}


def validate_placement_id(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates the ID field for placements.

    Optional on creation (Trafficking Status = 'New').
    Required on edit (Trafficking Status = 'Update').

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    placement_id = row.get("Placement ID")

    is_empty = _is_empty(placement_id)

    if status == "update" and is_empty:
        return {
            "row": row_num,
            "field": "Placement ID",
            "error": (
                "Placement ID is required when Trafficking Status is 'Update'."
            ),
        }
    return None


def validate_placement_name(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates the Name field for placements.

    Required on creation (Trafficking Status = 'New') and must be <= 512 chars.
    Optional on edit (Trafficking Status = 'Update').

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    placement_name = row.get("Placement Name")

    is_empty = _is_empty(placement_name)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Placement Name",
            "error": (
                "Placement Name is required when Trafficking Status is 'New'."
            ),
        }
    if status == "new" and len(str(placement_name)) > MAX_PLACEMENT_NAME_LENGTH:
        return {
            "row": row_num,
            "field": "Placement Name",
            "error": (
                "Placement Name must be less than or equal to 512 characters."
            ),
        }
    return None


def validate_profile_id(
    row: pd.Series,
    row_num: int,
    profile_id: str | None = None,
) -> dict[str, Any] | None:
    """Validates the Profile ID.

    Required for both 'New' and 'Update' trafficking status.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.
        profile_id: The profile ID parsed from sheet metadata.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    row_profile_id = row.get("Profile ID")
    if _is_empty(row_profile_id):
        row_profile_id = profile_id

    if _is_empty(row_profile_id):
        return {
            "row": row_num,
            "field": "Profile ID",
            "error": "Profile ID is required.",
        }
    return None


def validate_advertiser_id(
    row: pd.Series,
    row_num: int,
    advertiser_id: str | None = None,
) -> dict[str, Any] | None:
    """Validates the Advertiser ID.

    Required for both 'New' and 'Update' trafficking status.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.
        advertiser_id: The advertiser ID parsed from sheet metadata.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    row_advertiser_id = row.get("Advertiser ID")
    if _is_empty(row_advertiser_id):
        row_advertiser_id = advertiser_id

    if _is_empty(row_advertiser_id):
        return {
            "row": row_num,
            "field": "Advertiser ID",
            "error": "Advertiser ID is required.",
        }
    return None


def validate_campaign_id(
    row: pd.Series,
    row_num: int,
    campaign_id: str | None = None,
) -> dict[str, Any] | None:
    """Validates the Campaign ID.

    Required for both 'New' and 'Update' trafficking status.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.
        campaign_id: The campaign ID parsed from sheet metadata.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    row_campaign_id = row.get("Campaign ID")
    if _is_empty(row_campaign_id):
        row_campaign_id = campaign_id

    if _is_empty(row_campaign_id):
        return {
            "row": row_num,
            "field": "Campaign ID",
            "error": "Campaign ID is required.",
        }
    return None


def validate_site_id(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates the Site ID field for placements.

    Required on creation (Trafficking Status = 'New').

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    site_id = row.get("Site ID")

    is_empty = _is_empty(site_id)

    if is_empty:
        return {
            "row": row_num,
            "field": "Site ID",
            "error": "Site ID is required.",
        }
    return None


def validate_payment_source(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates the Payment Source field for placements.

    Required on creation (Trafficking Status = 'New').

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    payment_source = row.get("Placement Payment Source")

    is_empty = _is_empty(payment_source)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Payment Source",
            "error": (
                "Payment Source is required when Trafficking Status is 'New'."
            ),
        }
    return None


def validate_compatibility(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates placement compatibility on insertion.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    compatibility = row.get("Compatibility")
    if _is_empty(compatibility):
        compatibility = row.get("Placement Type")
    is_empty = _is_empty(compatibility)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Compatibility",
            "error": (
                "Compatibility is required when Trafficking Status is 'New'."
            ),
        }

    if status == "new":
        val = str(compatibility).strip().upper()
        if val in {"APP", "APP_INTERSTITIAL"}:
            return {
                "row": row_num,
                "field": "Compatibility",
                "error": (
                    "APP and APP_INTERSTITIAL are no longer allowed. Use"
                    " DISPLAY or DISPLAY_INTERSTITIAL instead."
                ),
            }

        if val not in {
            "DISPLAY",
            "DISPLAY_INTERSTITIAL",
            "IN_STREAM_VIDEO",
            "IN_STREAM_AUDIO",
        }:
            return {
                "row": row_num,
                "field": "Compatibility",
                "error": (
                    "Invalid compatibility value. Must be DISPLAY,"
                    " DISPLAY_INTERSTITIAL, IN_STREAM_VIDEO, or"
                    " IN_STREAM_AUDIO."
                ),
            }

    return None


def validate_placement_size(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates placement size on insertion.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    size = row.get("Placement Size")
    is_empty = _is_empty(size)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Placement Size",
            "error": (
                "Placement Size is required when Trafficking Status is 'New'."
            ),
        }
    return None


def validate_pricing_schedule(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates placement pricing schedule on insertion.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    start_date = row.get("Pricing Schedule Start Date")
    end_date = row.get("Pricing Schedule End Date")
    pricing_type = row.get("Pricing Schedule Type")

    missing = []
    if _is_empty(start_date):
        missing.append("startDate")
    if _is_empty(end_date):
        missing.append("endDate")
    if _is_empty(pricing_type):
        missing.append("pricingType")

    if status == "new" and missing:
        return {
            "row": row_num,
            "field": "Pricing Schedule",
            "error": (
                "Pricing Schedule subfields"
                f" ({', '.join(missing)}) are required on insertion."
            ),
        }
    return None


def validate_tag_formats(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates placement tag formats on insertion.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    tag_formats_val = row.get("Placement Tag Formats")
    is_empty = _is_empty(tag_formats_val)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Placement Tag Formats",
            "error": (
                "Placement Tag Formats is required when Trafficking"
                " Status is 'New'."
            ),
        }

    if status == "new":
        allowed_tag_formats = {
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
        }

        formats = [
            x.strip() for x in str(tag_formats_val).split(",") if x.strip()
        ]

        invalid_formats = [f for f in formats if f not in allowed_tag_formats]
        if invalid_formats:
            return {
                "row": row_num,
                "field": "Placement Tag Formats",
                "error": (
                    "Invalid placement tag format values:"
                    f" {', '.join(invalid_formats)}."
                ),
            }

    return None


def validate_ad_id(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates ad ID on edit.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    ad_id = row.get("Ad ID")

    is_empty = _is_empty(ad_id)

    if status == "update" and is_empty:
        return {
            "row": row_num,
            "field": "Ad ID",
            "error": "Ad ID is required when Trafficking Status is 'Update'.",
        }
    return None


def validate_ad_name(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates ad name on insertion.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    ad_name = row.get("Ad Name")
    is_empty = _is_empty(ad_name)

    if status == "new" and is_empty:
        return {
            "row": row_num,
            "field": "Ad Name",
            "error": "Ad Name is required when Trafficking Status is 'New'.",
        }
    if status == "new" and len(str(ad_name)) > MAX_AD_NAME_LENGTH:
        return {
            "row": row_num,
            "field": "Ad Name",
            "error": "Ad Name must be less than or equal to 256 characters.",
        }
    return None


def parse_date(val: str | float | None) -> datetime.date | None:
    """Parses a date string safely.

    Args:
        val: The date value.

    Returns:
        A datetime.date object if parsed successfully, otherwise None.
    """
    if _is_empty(val):
        return None
    try:
        return pd.to_datetime(str(val).strip()).date()
    except (ValueError, TypeError, KeyError):
        return None


def validate_ad_start_time(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates ad start time.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    start_val = row.get("Ad Start Date")

    is_empty = _is_empty(start_val)

    if is_empty:
        return {
            "row": row_num,
            "field": "Ad Start Date",
            "error": (
                "Ad Start Date is required when Trafficking Status is 'New' or"
                " 'Update'."
            ),
        }

    if not is_empty:
        start_date = parse_date(start_val)
        if start_date is None:
            return {
                "row": row_num,
                "field": "Ad Start Date",
                "error": f"Ad Start Date '{start_val}' is not a valid date.",
            }
        if start_date < datetime.date.today():  # ruff: ignore[call-date-today]
            return {
                "row": row_num,
                "field": "Ad Start Date",
                "error": f"Ad Start Date '{start_val}' cannot be in the past.",
            }
    return None


def validate_ad_end_time(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates ad end time.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    end_val = row.get("Ad End Date")
    start_val = row.get("Ad Start Date")

    is_empty = _is_empty(end_val)

    if is_empty:
        return {
            "row": row_num,
            "field": "Ad End Date",
            "error": (
                "Ad End Date is required when Trafficking Status is 'New' or"
                " 'Update'."
            ),
        }

    if not is_empty:
        end_date = parse_date(end_val)
        if end_date is None:
            return {
                "row": row_num,
                "field": "Ad End Date",
                "error": f"Ad End Date '{end_val}' is not a valid date.",
            }

        if not _is_empty(start_val):
            start_date = parse_date(start_val)
            if start_date is not None and end_date <= start_date:
                return {
                    "row": row_num,
                    "field": "Ad End Date",
                    "error": (
                        f"Ad End Date '{end_val}' must be later than Ad Start"
                        f" Date '{start_val}'."
                    ),
                }
    return None


def validate_placement_assignment(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates placement assignment on insertion and edit.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    placement_id = row.get("Placement ID")
    placement_name = row.get("Placement Name")

    has_id = not _is_empty(placement_id)
    has_name = not _is_empty(placement_name)

    if not (has_id or has_name):
        return {
            "row": row_num,
            "field": "Placement ID",
            "error": "Placement ID is required for placement assignment.",
        }

    return None


def validate_ad_type(row: pd.Series, row_num: int) -> dict[str, Any] | None:
    """Validates ad type.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status = str(row.get("Trafficking Status", "")).strip().lower()
    ad_type = row.get("Ad Type")
    is_empty = _is_empty(ad_type)

    if is_empty:
        return {
            "row": row_num,
            "field": "Ad Type",
            "error": (
                "Ad Type is required when Trafficking Status is 'New' or"
                " 'Update'."
            ),
        }
    if status == "new":
        val = str(ad_type).strip().upper()
        if val == "AD_SERVING_DEFAULT_AD":
            return {
                "row": row_num,
                "field": "Ad Type",
                "error": (
                    "Default ads (AD_SERVING_DEFAULT_AD) cannot be created"
                    " directly using the API."
                ),
            }
        allowed = {
            "AD_SERVING_STANDARD_AD",
            "AD_SERVING_CLICK_TRACKER",
            "AD_SERVING_TRACKING",
            "AD_SERVING_BRAND_SAFE_AD",
        }
        if val not in allowed:
            return {
                "row": row_num,
                "field": "Ad Type",
                "error": (
                    f"Invalid Ad Type '{ad_type}'. Must be one of"
                    f" {sorted(allowed)}."
                ),
            }
    return None


def validate_ad_delivery_schedule(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates ad delivery schedule.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    ad_type = str(row.get("Ad Type", "")).strip().upper()

    if ad_type not in {"AD_SERVING_STANDARD_AD", "AD_SERVING_TRACKING"}:
        return None

    if ad_type == "AD_SERVING_STANDARD_AD":
        # Priority
        priority = row.get("Delivery Schedule Priority")
        if _is_empty(priority):
            priority = row.get("Priority")

        # Impression Ratio
        ratio = row.get("Delivery Schedule Impression Ratio")
        if _is_empty(ratio):
            ratio = row.get("Impression Ratio")

        has_priority = not _is_empty(priority)
        has_ratio = not _is_empty(ratio)

        if not (has_priority and has_ratio):
            missing = []
            if not has_priority:
                missing.append("priority")
            if not has_ratio:
                missing.append("impressionRatio")
            return {
                "row": row_num,
                "field": "Delivery Schedule",
                "error": (
                    f"Delivery Schedule subfields ({', '.join(missing)}) are"
                    " required when Ad Type is AD_SERVING_STANDARD_AD."
                ),
            }

    return None


def validate_ad_dynamic_click_tracker(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates dynamic click tracker boolean field for click tracker ads.

    Required for both 'New' and 'Update' trafficking status when Ad Type is
    AD_SERVING_CLICK_TRACKER. Must strictly be 'TRUE' or 'FALSE'.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    ad_type = str(row.get("Ad Type", "")).strip().upper()
    if ad_type != "AD_SERVING_CLICK_TRACKER":
        return None

    val = row.get("Ad Dynamic Click Tracker")
    if _is_empty(val):
        return {
            "row": row_num,
            "field": "Ad Dynamic Click Tracker",
            "error": (
                "Ad Dynamic Click Tracker is required when Ad Type is"
                " AD_SERVING_CLICK_TRACKER."
            ),
        }

    val_str = str(val).strip().upper()
    if val_str not in {"TRUE", "FALSE"}:
        return {
            "row": row_num,
            "field": "Ad Dynamic Click Tracker",
            "error": (
                f"Invalid Ad Dynamic Click Tracker value '{val}'. Must be"
                " 'TRUE' or 'FALSE'."
            ),
        }

    return None


def validate_click_tracker_url(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates destination URL for click tracker ads.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    ad_type = str(row.get("Ad Type", "")).strip().upper()
    if ad_type != "AD_SERVING_CLICK_TRACKER":
        return None

    final_url = row.get("Final Trafficking URL")
    if _is_empty(final_url):
        return {
            "row": row_num,
            "field": "Final Trafficking URL",
            "error": "Final Trafficking URL is required for click tracker ads.",
        }

    return None


def validate_creative_rotation(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates ad creative rotation assignment.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    ad_type = str(row.get("Ad Type", "")).strip().upper()
    if ad_type == "AD_SERVING_CLICK_TRACKER":
        return None

    creative_name = row.get("Creative Name")
    is_empty_name = _is_empty(creative_name)

    if is_empty_name:
        return {
            "row": row_num,
            "field": "Creative Name",
            "error": "Creative Name is required for creative assignment.",
        }

    final_url = row.get("Final Trafficking URL")
    is_empty_url = _is_empty(final_url)

    if is_empty_url:
        return {
            "row": row_num,
            "field": "Final Trafficking URL",
            "error": (
                "Final Trafficking URL is required for creative assignment."
            ),
        }

    return None


def validate_placement_status(
    row: pd.Series, row_num: int
) -> dict[str, Any] | None:
    """Validates placement status.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A dictionary with error details if validation fails, otherwise None.
    """
    status_val = row.get("Placement Status")

    if not _is_empty(status_val):
        val = str(status_val).strip().upper()
        allowed = {
            "PLACEMENT_STATUS_ACTIVE",
            "PLACEMENT_STATUS_ARCHIVED",
            "PLACEMENT_STATUS_INACTIVE",
            "PLACEMENT_STATUS_PERMANENTLY_ARCHIVED",
            "PLACEMENT_STATUS_UNKNOWN",
        }
        if val not in allowed:
            return {
                "row": row_num,
                "field": "Placement Status",
                "error": (
                    f"Invalid placement status '{status_val}'. Must be one of"
                    f" {sorted(allowed)}."
                ),
            }
    return None


VALID_EVENT_TAG_TYPES = {
    "CLICK_THROUGH_EVENT_TAG",
    "IMPRESSION_IMAGE_EVENT_TAG",
    "IMPRESSION_JAVASCRIPT_EVENT_TAG",
}

VALID_EVENT_TAG_STATUSES = {
    "ENABLED",
    "DISABLED",
}


def validate_event_tags(row: pd.Series, row_num: int) -> list[dict[str, Any]]:
    """Validates comma-separated event tag fields in a row.

    Ensures that event tag names, types, and URLs have matching counts
    and valid values.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.
        row_num: The 1-based row number in the spreadsheet.

    Returns:
        A list of dictionary errors if validation fails.
    """
    errors: list[dict[str, Any]] = []

    names_val = row.get("Event Tag Names")
    types_val = row.get("Event Tag Types")
    urls_val = row.get("Event Tag Urls")
    status_val = row.get("Event Tag Status")

    has_names = not _is_empty(names_val)
    has_types = not _is_empty(types_val)
    has_urls = not _is_empty(urls_val)
    has_status = not _is_empty(status_val)

    if not has_names and not has_types and not has_urls and not has_status:
        return errors

    if not has_names and (has_types or has_urls or has_status):
        errors.append({
            "row": row_num,
            "field": "Event Tag Names",
            "error": (
                "Event Tag Names is required when Event Tag Types, Urls, or"
                " Status are provided. All 4 Event Tags Information columns"
                " ('Event Tag Names', 'Event Tag Types', 'Event Tag Urls',"
                " 'Event Tag Status') must have matching entries."
            ),
        })
        return errors

    names = [n.strip() for n in str(names_val).split(",") if n.strip()]
    if not names:
        errors.append({
            "row": row_num,
            "field": "Event Tag Names",
            "error": "Event Tag Names cannot be empty.",
        })
        return errors

    if not has_types:
        errors.append({
            "row": row_num,
            "field": "Event Tag Types",
            "error": (
                "Event Tag Types is required when Event Tag Names is"
                " provided. All 4 Event Tags Information columns"
                " ('Event Tag Names', 'Event Tag Types', 'Event Tag Urls',"
                " 'Event Tag Status') must have the same number of"
                " comma-separated entries."
            ),
        })
    else:
        types = [t.strip() for t in str(types_val).split(",") if t.strip()]
        if len(types) != len(names):
            errors.append({
                "row": row_num,
                "field": "Event Tag Types",
                "error": (
                    f"Mismatch in count of Event Tag Names ({len(names)}) and"
                    f" Event Tag Types ({len(types)}). All 4 Event Tags"
                    " Information columns ('Event Tag Names',"
                    " 'Event Tag Types',"
                    " 'Event Tag Urls', 'Event Tag Status') must have the same"
                    " number of comma-separated entries."
                ),
            })
        errors.extend([
            {
                "row": row_num,
                "field": "Event Tag Types",
                "error": (
                    f"Invalid Event Tag Type '{t}'. Must be one of"
                    f" {sorted(VALID_EVENT_TAG_TYPES)}."
                ),
            }
            for t in types
            if t.upper() not in VALID_EVENT_TAG_TYPES
        ])

    if not has_urls:
        errors.append({
            "row": row_num,
            "field": "Event Tag Urls",
            "error": (
                "Event Tag Urls is required when Event Tag Names is"
                " provided. All 4 Event Tags Information columns"
                " ('Event Tag Names', 'Event Tag Types', 'Event Tag Urls',"
                " 'Event Tag Status') must have the same number of"
                " comma-separated entries."
            ),
        })
    else:
        urls = [u.strip() for u in str(urls_val).split(",") if u.strip()]
        if len(urls) != len(names):
            errors.append({
                "row": row_num,
                "field": "Event Tag Urls",
                "error": (
                    f"Mismatch in count of Event Tag Names ({len(names)}) and"
                    f" Event Tag Urls ({len(urls)}). All 4 Event Tags"
                    " Information columns ('Event Tag Names',"
                    " 'Event Tag Types',"
                    " 'Event Tag Urls', 'Event Tag Status') must have the same"
                    " number of comma-separated entries."
                ),
            })

    if has_status:
        statuses = [s.strip() for s in str(status_val).split(",") if s.strip()]
        if len(statuses) != len(names):
            errors.append({
                "row": row_num,
                "field": "Event Tag Status",
                "error": (
                    f"Mismatch in count of Event Tag Names ({len(names)}) and"
                    f" Event Tag Status ({len(statuses)}). All 4 Event Tags"
                    " Information columns ('Event Tag Names',"
                    " 'Event Tag Types',"
                    " 'Event Tag Urls', 'Event Tag Status') must have the same"
                    " number of comma-separated entries."
                ),
            })
        errors.extend([
            {
                "row": row_num,
                "field": "Event Tag Status",
                "error": (
                    f"Invalid Event Tag Status '{s}'. Must be one of"
                    f" {sorted(VALID_EVENT_TAG_STATUSES)}."
                ),
            }
            for s in statuses
            if s.upper() not in VALID_EVENT_TAG_STATUSES
        ])

    return errors


def validate_sheet(  # ruff: ignore[complex-structure, too-many-branches]
    df: pd.DataFrame,
    profile_id: str | None = None,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
) -> list[dict[str, Any]]:
    """Runs all validations on the trafficking sheet DataFrame.

    Args:
        df: Cleaned DataFrame containing campaign trafficking data.
        profile_id: Profile ID from metadata block or column.
        advertiser_id: Advertiser ID from metadata block or column.
        campaign_id: Campaign ID from metadata block or column.

    Returns:
        A list of validation error dicts.
    """
    errors = []

    for _, row in df.iterrows():
        row_num = int(row.get("Row Number", 0))

        err = validate_profile_id(row, row_num, profile_id)
        if err:
            errors.append(err)

        err = validate_advertiser_id(row, row_num, advertiser_id)
        if err:
            errors.append(err)

        err = validate_campaign_id(row, row_num, campaign_id)
        if err:
            errors.append(err)

        # Run ad validations
        err = validate_ad_name(row, row_num)
        if err:
            errors.append(err)

        err = validate_ad_start_time(row, row_num)
        if err:
            errors.append(err)

        err = validate_ad_end_time(row, row_num)
        if err:
            errors.append(err)

        err = validate_ad_type(row, row_num)
        if err:
            errors.append(err)

        err = validate_ad_delivery_schedule(row, row_num)
        if err:
            errors.append(err)

        err = validate_ad_dynamic_click_tracker(row, row_num)
        if err:
            errors.append(err)

        err = validate_click_tracker_url(row, row_num)
        if err:
            errors.append(err)

        err = validate_creative_rotation(row, row_num)
        if err:
            errors.append(err)

        # Run event tag validations
        event_tag_errs = validate_event_tags(row, row_num)
        if event_tag_errs:
            errors.extend(event_tag_errs)

    return errors
