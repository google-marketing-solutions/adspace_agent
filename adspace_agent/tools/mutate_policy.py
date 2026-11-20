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
"""Mutation policy enforcement, account allowlist, and confirmation gate."""

import re
from typing import Any

from google.adk.tools.base_tool import BaseTool
from google.adk.tools.tool_context import ToolContext

from adspace_agent.config import get_allowed_mutate_account_ids
from adspace_agent.config import normalize_account_id
from adspace_agent.tools.untrusted_content import UNTRUSTED_TURN_INVOCATION_KEY

_NON_MUTATING_INTERNAL_TOOLS: frozenset[str] = frozenset({
    "generate_image",
    "generate_video",
    "get_info_about_youtube_video",
    "parse_sheet_tool",
    "list_user_files",
    "reload_skills",
})

_READONLY_POST_SUFFIXES: tuple[str, ...] = (
    ".search",
    ":search",
    "_search",
    ".query",
    ":query",
    "_query",
    "generatereachforecast",
    "generate_reach_forec",
    "generateaudiencedefinition",
    "generate_audience_definition",
    "generateaudiencecompositioninsights",
    "generate_audience_composition_insights",
    ".audit",
    "_audit",
)

_MUTATING_NAME_PATTERN: re.Pattern[str] = re.compile(
    r"(?:^|_)(?:create|insert|batchinsert|patch|update|batchupdate|"
    r"bulk_create|bulk_update|bulk_edit|delete|remove|upload|edit|"
    r"replace|duplicate|publish|retransform|mutate|run)(?:_|$)"
)

_ACCOUNT_ID_KEYS: frozenset[str] = frozenset({
    "advertiserid",
    "customerid",
    "accountid",
    "profileid",
    "partnerid",
    "merchantid",
    "logincustomerid",
})

_RESOURCE_NAME_KEYS: frozenset[str] = frozenset({
    "name",
    "parent",
    "resourcename",
})

_RESOURCE_ID_PATTERN: re.Pattern[str] = re.compile(
    r"(?:^|/)(?:advertisers|customers|accounts|partners|userProfiles)/"
    r"([A-Za-z0-9_-]+)"
)


def _is_readonly_post_operation(tool_name: str, operation_id: object) -> bool:
    """Checks whether a non-GET operation matches known read-only queries.

    Args:
        tool_name: Lowercased ADK tool name.
        operation_id: Optional OpenAPI operationId attribute.

    Returns:
        True if the operation matches a known read-only query/search suffix.
    """
    if tool_name.endswith(_READONLY_POST_SUFFIXES):
        return True
    return isinstance(operation_id, str) and (
        operation_id.lower().endswith(_READONLY_POST_SUFFIXES)
    )


def is_mutating_tool(tool: BaseTool) -> bool:
    """Determines whether an ADK tool performs an external write or mutation.

    Args:
        tool: The ADK tool instance about to be invoked.

    Returns:
        True if the tool writes, mutates, or executes external changes, else
        False.
    """
    tool_name = (getattr(tool, "name", "") or "").lower()
    if (
        tool_name == "traffic_campaigns_in_cm360_tool"
        or "upload_script" in tool_name
        or "upload_rules" in tool_name
    ):
        return True

    if tool_name in _NON_MUTATING_INTERNAL_TOOLS:
        return False

    rest_tool = getattr(tool, "_rest_api_tool", tool)
    endpoint = getattr(rest_tool, "endpoint", None)
    method = getattr(endpoint, "method", None)
    if isinstance(method, str):
        upper_method = method.strip().upper()
        if upper_method in {"GET", "HEAD", "OPTIONS"}:
            return False
        if upper_method in {"POST", "PUT", "PATCH", "DELETE"}:
            operation = getattr(rest_tool, "operation", None)
            operation_id = getattr(operation, "operationId", None)
            return not _is_readonly_post_operation(tool_name, operation_id)
        return True

    return bool(_MUTATING_NAME_PATTERN.search(tool_name))


def _extract_scalar_account_ids(
    value: object, found: set[str], current_key: str | None
) -> None:
    """Extracts account IDs from a scalar argument value."""
    if current_key in _ACCOUNT_ID_KEYS:
        if isinstance(value, int) and not isinstance(value, bool):
            found.add(str(value))
        elif isinstance(value, str) and (
            normalized := normalize_account_id(value)
        ):
            found.add(normalized)
        return

    if current_key in _RESOURCE_NAME_KEYS and isinstance(value, str):
        for match in _RESOURCE_ID_PATTERN.finditer(value):
            if normalized := normalize_account_id(match.group(1)):
                found.add(normalized)


def _collect_account_ids(
    value: object, found: set[str], current_key: str | None = None
) -> None:
    """Recursively extracts normalized account IDs from tool arguments."""
    if isinstance(value, dict):
        for key, nested in value.items():
            norm_key = str(key).lower().replace("_", "")
            _collect_account_ids(nested, found, norm_key)
        return

    if isinstance(value, (list, tuple)):
        for item in value:
            _collect_account_ids(item, found, current_key)
        return

    _extract_scalar_account_ids(value, found, current_key)


def extract_target_account_ids(args: dict[str, Any]) -> set[str]:
    """Extracts normalized target account IDs from tool arguments.

    Args:
        args: Arguments passed to the tool invocation.

    Returns:
        A set of normalized account identifiers discovered in ``args``.
    """
    found: set[str] = set()
    _collect_account_ids(args, found)
    return found


def _check_account_allowlist(
    tool_name: str, args: dict[str, Any]
) -> dict[str, Any] | None:
    """Validates target account IDs against ``ALLOWED_MUTATE_ACCOUNT_IDS``.

    Args:
        tool_name: Name of the tool being invoked.
        args: Arguments supplied to the tool invocation.

    Returns:
        An error dictionary if the allowlist rejects the call, else None.
    """
    allowed_ids = get_allowed_mutate_account_ids()
    if allowed_ids is None:
        return None

    target_ids = extract_target_account_ids(args)
    if not target_ids:
        return {
            "error": (
                f"Mutation blocked for '{tool_name}': target account ID "
                "could not be determined for allowlist verification."
            )
        }

    disallowed = sorted(target_ids - allowed_ids)
    if disallowed:
        disallowed_str = ", ".join(disallowed)
        return {
            "error": (
                f"Mutation blocked for '{tool_name}': target account ID(s) "
                f"{disallowed_str} are not in ALLOWED_MUTATE_ACCOUNT_IDS."
            )
        }
    return None


def enforce_mutate_tool_policy_callback(
    tool: BaseTool,
    args: dict[str, Any],
    tool_context: ToolContext,
) -> dict[str, Any] | None:
    """ADK before_tool_callback gating mutating marketing API tools.

    Enforces account allowlists, blocks mutations in turns tainted by untrusted
    external content, and requires explicit ADK ToolConfirmation approval before
    executing mutating tools.

    Args:
        tool: The ADK tool being invoked.
        args: Arguments supplied to the tool.
        tool_context: Active ADK tool context.

    Returns:
        An error dictionary blocking execution or requesting confirmation, or
        None to allow execution to proceed.
    """
    if not is_mutating_tool(tool):
        return None

    allowlist_error = _check_account_allowlist(tool.name, args)
    if allowlist_error is not None:
        return allowlist_error

    confirmation = getattr(tool_context, "tool_confirmation", None)
    if confirmation is not None:
        if confirmation.confirmed:
            return None
        return {
            "error": f"Tool call '{tool.name}' was rejected by the user.",
        }

    invocation_id = getattr(tool_context, "invocation_id", None)
    state = getattr(tool_context, "state", None)
    if (
        isinstance(invocation_id, str)
        and invocation_id
        and state is not None
        and state.get(UNTRUSTED_TURN_INVOCATION_KEY) == invocation_id
    ):
        return {
            "error": (
                f"Mutation blocked for '{tool.name}': mutating tool calls are "
                "not permitted in the same turn as untrusted external content. "
                "Ask the user to submit the change request directly in a new "
                "turn."
            )
        }

    tool_context.request_confirmation(
        hint=f"Approve executing mutating tool '{tool.name}'?",
        payload={"tool": tool.name, "args": args},
    )
    actions = getattr(tool_context, "actions", None)
    if actions is not None:
        actions.skip_summarization = True

    return {
        "error": (
            f"Tool '{tool.name}' modifies external resources and requires "
            "user confirmation before execution."
        )
    }
