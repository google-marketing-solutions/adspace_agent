# Copyright 2025 Google LLC
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
"""A set of tools for the AdSpace Agent to interact with Google Ads."""

from collections.abc import Iterable, MutableSequence
from typing import cast, Optional, override, TypedDict

from dotenv import load_dotenv
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.tools import BaseTool
from google.adk.tools import FunctionTool
from google.adk.tools.base_toolset import BaseToolset
from google.ads.googleads.client import GoogleAdsClient
from google.ads.googleads.v21.resources.types.customer_client import (
    CustomerClient,
)
from google.ads.googleads.v21.services.services.customer_service.client import (
    CustomerServiceClient,
)
from google.ads.googleads.v21.services.services.google_ads_service import (
    GoogleAdsServiceClient,
)
from google.ads.googleads.v21.services.services.google_ads_service.pagers import (  # noqa: E501
    SearchPager,
)
from google.ads.googleads.v21.services.types.customer_service import (
    ListAccessibleCustomersResponse,
)
from google.ads.googleads.v21.services.types.google_ads_service import (
    GoogleAdsRow,
)
from google.ads.googleads.v21.services.types.google_ads_service import (
    SearchGoogleAdsStreamResponse,
)

_ = load_dotenv()

client = GoogleAdsClient.load_from_env()


class CustomerNode(TypedDict):
    """A type for a customer node in a hierarchy."""

    id: int
    descriptive_name: str
    currency_code: str
    time_zone: str
    manager: bool
    children: list["CustomerNode"]


@FunctionTool
def account_hierarchy(
    login_customer_id: Optional[str] = None,
) -> dict[str, list[CustomerNode] | str]:
    """Builds a hierarchy of accounts under the given manager account.

    Args:
        login_customer_id (Optional[str]): The ID of the login customer.
            Defaults to None.

    Returns:
        A dictionary representing the account hierarchy.
    """
    try:
        if login_customer_id is not None:
            client.login_customer_id = login_customer_id
        else:
            client.login_customer_id = None

        googleads_service: GoogleAdsServiceClient = cast(
            GoogleAdsServiceClient, client.get_service("GoogleAdsService")
        )
        customer_service: CustomerServiceClient = cast(
            CustomerServiceClient, client.get_service("CustomerService")
        )

        seed_customer_ids: list[str] = []
        result: list[CustomerNode] = []

        # Creates a query that retrieves all child accounts of the manager
        # specified in search calls below.
        query = """
            SELECT
                customer_client.client_customer,
                customer_client.level,
                customer_client.manager,
                customer_client.descriptive_name,
                customer_client.currency_code,
                customer_client.time_zone,
                customer_client.id
            FROM customer_client
            WHERE customer_client.level <= 1"""

        if login_customer_id is not None:
            # Ensure login_customer_id is treated as a string for this list.
            seed_customer_ids = [login_customer_id]
        else:
            customer_resource_names: MutableSequence[str] = (
                customer_service.list_accessible_customers().resource_names
            )

            for customer_resource_name in customer_resource_names:
                customer_id_from_parse: str = (
                    googleads_service.parse_customer_path(
                        customer_resource_name
                    )["customer_id"]
                )
                seed_customer_ids.append(customer_id_from_parse)

        for seed_customer_id_str in seed_customer_ids:
            # Performs a breadth-first search to build a Dictionary that maps
            # managers to their child accounts (customer_ids_to_child_accounts).
            unprocessed_customer_ids: list[int] = [int(seed_customer_id_str)]
            customer_ids_to_child_accounts: dict[int, list[CustomerClient]] = {}
            root_customer_client: CustomerClient | None = None

            while unprocessed_customer_ids:
                customer_id: int = unprocessed_customer_ids.pop(0)
                response: SearchPager = googleads_service.search(
                    customer_id=str(customer_id), query=query
                )

                # Iterates over all rows in all pages to get all customer
                # clients under the specified customer's hierarchy.
                googleads_row: GoogleAdsRow
                for googleads_row in response:
                    customer_client_loop_var: CustomerClient = (
                        googleads_row.customer_client
                    )

                    # The customer client with level 0 is the specified
                    # customer.
                    if customer_client_loop_var.level == 0:
                        if root_customer_client is None:
                            root_customer_client = customer_client_loop_var
                        continue

                    # For all level-1 (direct child) accounts that are manager
                    # accounts, the above query will be run against them to
                    # create a map of managers to their child accounts.
                    if customer_id not in customer_ids_to_child_accounts:
                        customer_ids_to_child_accounts[customer_id] = []

                    customer_ids_to_child_accounts[customer_id].append(
                        customer_client_loop_var
                    )

                    if customer_client_loop_var.manager:
                        # A customer can be managed by multiple managers, so to
                        # prevent visiting the same customer many times, we need
                        # to check if it's already in the map.
                        if (
                            customer_client_loop_var.id
                            not in customer_ids_to_child_accounts
                            and customer_client_loop_var.level == 1
                        ):
                            unprocessed_customer_ids.append(
                                customer_client_loop_var.id
                            )

            if root_customer_client is not None:
                result.append(  # pyright:ignore[reportUnreachable]
                    _build_hierarchy_dict(
                        root_customer_client, customer_ids_to_child_accounts
                    )
                )
        return {
            "status": "SUCCESS",
            "data": result,
        }
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


def _build_hierarchy_dict(
    customer_client: CustomerClient,
    customer_ids_to_child_accounts: dict[int, list[CustomerClient]],
) -> CustomerNode:
    """Recursively builds a dictionary representing the account hierarchy.

    Args:
        customer_client: The customer client for the current node in the
            hierarchy.
        customer_ids_to_child_accounts: A map from customer IDs to their
            direct child accounts.

    Returns:
        A dictionary representing the subtree for the given customer client.
    """
    customer_client_id = customer_client.id
    node: CustomerNode = {
        "id": customer_client.id,
        "descriptive_name": customer_client.descriptive_name,
        "currency_code": customer_client.currency_code,
        "time_zone": customer_client.time_zone,
        "manager": customer_client.manager,
        "children": [],
    }

    if customer_client_id in customer_ids_to_child_accounts:
        for child_account in customer_ids_to_child_accounts[customer_client_id]:
            child_node = _build_hierarchy_dict(
                child_account, customer_ids_to_child_accounts
            )
            node["children"].append(child_node)

    return node


@FunctionTool
def list_accounts() -> dict[str, ListAccessibleCustomersResponse | str]:
    """Lists all accessible Google Ads accounts for the authenticated user.

    Returns:
        A dictionary representing the list of accessible accounts.
    """
    try:
        client.login_customer_id = None

        customer_service: CustomerServiceClient = cast(
            CustomerServiceClient, client.get_service("CustomerService")
        )
        accessible_customers: ListAccessibleCustomersResponse = (
            customer_service.list_accessible_customers()
        )
        return {
            "status": "SUCCESS",
            "data": type(accessible_customers).to_json(accessible_customers),
        }
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


@FunctionTool
def search_stream(
    customer_id: str,
    query: str,
    login_customer_id: Optional[str] = None,
) -> dict[str, list[str] | str]:
    """Streams Google Ads data from a GAQL query.

    Args:
        customer_id (str): The ID of the customer to search.
        query (str): The Google Ads Query Language query.
        login_customer_id (Optional[str]): The ID of the login customer.
            Defaults to None.

    Returns:
        A dictionary representing the streamed data.
    """
    try:
        if login_customer_id is not None:
            client.login_customer_id = login_customer_id
        else:
            client.login_customer_id = None

        service: GoogleAdsServiceClient = cast(
            GoogleAdsServiceClient, client.get_service("GoogleAdsService")
        )
        stream: Iterable[SearchGoogleAdsStreamResponse] = service.search_stream(
            customer_id=customer_id,
            query=query,
        )
        results = []
        for batch in stream:
            rows = batch.results
            for row in rows:
                results.append(type(row).to_json(row))
        return {
            "status": "SUCCESS",
            "data": results,
        }
    except Exception as ex:  # pylint: disable=broad-exception-caught
        return {
            "status": "ERROR",
            "error_details": str(ex),
        }


class GoogleAdsToolset(BaseToolset):
    """A custom toolset that groups all our Google Ads functions."""

    @override
    async def get_tools(  # pytype: disable=override-error
        self,
        readonly_context: ReadonlyContext | None = None,  # pylint: disable=unused-argument
    ) -> list[BaseTool]:
        return [list_accounts, search_stream, account_hierarchy]
