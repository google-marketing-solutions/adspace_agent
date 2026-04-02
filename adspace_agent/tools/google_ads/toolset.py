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

"""Toolset for Google Ads API."""

import logging
from typing import Any

from google.adk.auth.auth_credential import AuthCredential
from google.adk.auth.auth_credential import AuthCredentialTypes
from google.adk.auth.auth_credential import OAuth2Auth
from google.adk.auth.auth_schemes import OpenIdConnectWithConfig
from google.adk.tools.base_toolset import BaseToolset
from google.adk.tools.base_toolset import ToolPredicate
from google.adk.tools.openapi_tool import OpenAPIToolset

from .discovery_converter import DiscoveryConverter

logger = logging.getLogger(__name__)

GOOGLE_ADS_API_VERSION = "v23"


class GoogleAdsToolset(BaseToolset):
    """Google Ads Toolset dynamically generated from the discovery doc."""

    def __init__(  # noqa: PLR0913, PLR0917
        self,
        developer_token: str,
        client_id: str,
        client_secret: str,
        discovery_doc_url: str = f"https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}",
        login_customer_id: str | None = None,
        tool_filter: ToolPredicate | list[str] | None = None,
    ) -> None:
        """Initializes the Toolset.

        Args:
            developer_token: Developer token.
            client_id: OAuth2 client ID.
            client_secret: OAuth2 client secret.
            discovery_doc_url: URL or path to discovery doc.
            login_customer_id: Optional login customer ID.
            tool_filter: Optional tool filter list or predicate.
        """
        super().__init__(tool_filter=tool_filter)

        self.developer_token = developer_token
        self.client_id = client_id
        self.client_secret = client_secret
        self.login_customer_id = login_customer_id

        # Generate OpenAPI spec dynamically from local discovery doc
        converter = DiscoveryConverter(discovery_doc_url)
        openapi_spec = converter.convert()

        if tool_filter is not None:
            pruned_paths = {}
            for path, methods in openapi_spec.get("paths", {}).items():
                kept_methods = {}
                for http_method, operation in methods.items():
                    operation_id = operation.get("operationId")
                    if operation_id:
                        keep = False
                        if isinstance(tool_filter, list):
                            keep = operation_id in tool_filter
                        elif callable(tool_filter):
                            keep = tool_filter(operation_id)

                        if keep:
                            kept_methods[http_method] = operation
                if kept_methods:
                    pruned_paths[path] = kept_methods
            openapi_spec["paths"] = pruned_paths

        def header_provider(context: Any) -> dict[str, str]:  # noqa: ARG001, ANN401
            headers: dict[str, str] = {}
            headers["developer-token"] = self.developer_token
            if self.login_customer_id:
                headers["login-customer-id"] = self.login_customer_id
            return headers

        auth_scheme = OpenIdConnectWithConfig(
            authorization_endpoint="https://accounts.google.com/o/oauth2/v2/auth",
            token_endpoint="https://oauth2.googleapis.com/token",  # noqa: S106
            userinfo_endpoint="https://openidconnect.googleapis.com/v1/userinfo",
            revocation_endpoint="https://oauth2.googleapis.com/revoke",
            token_endpoint_auth_methods_supported=[
                "client_secret_post",
                "client_secret_basic",
            ],
            grant_types_supported=["authorization_code"],
            scopes=["https://www.googleapis.com/auth/adwords"],
        )

        auth_credential = AuthCredential(
            auth_type=AuthCredentialTypes.OPEN_ID_CONNECT,
            oauth2=OAuth2Auth(
                client_id=self.client_id,
                client_secret=self.client_secret,
            ),
        )

        self._openapi_toolset = OpenAPIToolset(
            spec_dict=openapi_spec,
            auth_scheme=auth_scheme,
            auth_credential=auth_credential,
            header_provider=header_provider,
            tool_filter=tool_filter,
            credential_key="google_ads_api",
        )

    async def get_tools(
        self,
        readonly_context: Any | None = None,  # noqa: ANN401
    ) -> list[Any]:
        """Return the precomputed list of ADK RestApiTools.

        Args:
            readonly_context: ReadonlyContext from ADK.

        Returns:
            A list of tools exposing Google Ads operations.
        """
        tools: list[Any] = []
        tools.extend(await self._openapi_toolset.get_tools(readonly_context))

        return tools

    def get_auth_config(self) -> Any | None:  # noqa: ANN401
        """Returns the auth config for this toolset.

        Returns:
            AuthConfig with the configured auth scheme and credential.
        """
        return self._openapi_toolset.get_auth_config()

    async def close(self) -> None:
        """Closes the underlying toolset."""
        await self._openapi_toolset.close()
