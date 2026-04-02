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

"""Google API Discovery Document converter."""

import json
import logging
import pathlib
import re
from typing import Any

import httpx

logger = logging.getLogger(__name__)


class DiscoveryConverter:
    """Converts a local Google API Discovery document to an OpenAPI v3 specification.

    Based on the Google ADK googleapi_to_openapi_converter.py.
    """

    def __init__(self, discovery_doc_url: str) -> None:
        """Initializes the converter.

        Args:
            discovery_doc_url: URL or path to discovery doc.
        """
        self.discovery_doc_url = discovery_doc_url
        self._google_api_spec: dict[str, Any] = {}
        self._openapi_spec: dict[str, Any] = {
            "openapi": "3.0.0",
            "info": {},
            "servers": [],
            "paths": {},
            "components": {"schemas": {}, "securitySchemes": {}},
        }

    def convert(self) -> dict[str, Any]:
        """Convert the local Google API spec to OpenAPI v3 format.

        Returns:
            dict[str, Any]: OpenAPI spec.
        """
        if self.discovery_doc_url.startswith("http"):
            response = httpx.get(self.discovery_doc_url, timeout=30.0)
            response.raise_for_status()
            self._google_api_spec = response.json()
        else:
            with pathlib.Path(self.discovery_doc_url).open(
                "r", encoding="utf-8"
            ) as f:
                self._google_api_spec = json.load(f)

        self._convert_info()
        self._convert_servers()
        self._convert_security_schemes()
        self._convert_schemas()
        self._convert_resources(self._google_api_spec.get("resources", {}))
        self._convert_methods(self._google_api_spec.get("methods", {}))

        return self._openapi_spec

    def _convert_info(self) -> None:
        self._openapi_spec["info"] = {
            "title": self._google_api_spec.get("title", "Google Ads API"),
            "description": self._google_api_spec.get("description", ""),
            "version": self._google_api_spec.get("version", ""),
        }

    def _convert_servers(self) -> None:
        base_url = self._google_api_spec.get(
            "rootUrl", ""
        ) + self._google_api_spec.get("servicePath", "")
        base_url = base_url.removesuffix("/")

        self._openapi_spec["servers"] = [
            {
                "url": base_url,
                "description": f"Google Ads {self._google_api_spec.get('version')} API",
            }
        ]

    def _convert_security_schemes(self) -> None:
        self._openapi_spec["components"]["securitySchemes"]["oauth2"] = {
            "type": "oauth2",
            "description": "OAuth 2.0 authentication",
            "flows": {
                "authorizationCode": {
                    "authorizationUrl": "https://accounts.google.com/o/oauth2/auth",
                    "tokenUrl": "https://oauth2.googleapis.com/token",
                    "scopes": {
                        "https://www.googleapis.com/auth/adwords": "Manage your AdWords campaigns"
                    },
                }
            },
        }

    def _convert_schemas(self) -> None:
        schemas = self._google_api_spec.get("schemas", {})
        for schema_name, schema_def in schemas.items():
            self._openapi_spec["components"]["schemas"][schema_name] = (
                self._convert_schema_object(schema_def)
            )

    def _convert_schema_object(
        self, schema_def: dict[str, Any]
    ) -> dict[str, Any]:
        result = {}
        if "type" in schema_def:
            gtype = schema_def["type"]
            if gtype == "object":
                result["type"] = "object"
                if "properties" in schema_def:
                    result["properties"] = {}
                    for prop_name, prop_def in schema_def["properties"].items():
                        result["properties"][prop_name] = (
                            self._convert_schema_object(prop_def)
                        )
                required_fields = [
                    k
                    for k, v in schema_def.get("properties", {}).items()
                    if v.get("required", False)
                ]
                if required_fields:
                    result["required"] = required_fields
            elif gtype == "array":
                result["type"] = "array"
                if "items" in schema_def:
                    result["items"] = self._convert_schema_object(
                        schema_def["items"]
                    )
            elif gtype == "any":
                result["type"] = (
                    "object"  # Simplification for ADK since oneOf can cause schema bloat
                )
            else:
                result["type"] = gtype

        if "$ref" in schema_def:
            ref = schema_def["$ref"]
            if ref.startswith("#"):
                ref = ref.replace("#", "#/components/schemas/")
            else:
                ref = "#/components/schemas/" + ref
            result["$ref"] = ref

        for attr in ["format", "enum", "description", "pattern", "default"]:
            if attr in schema_def:
                result[attr] = schema_def[attr]

        return result

    def _convert_resources(
        self, resources: dict[str, Any], parent_path: str = ""
    ) -> None:
        for resource_name, resource_data in resources.items():
            resource_path = (
                f"{parent_path}/{resource_name}"
                if parent_path != "/"
                else f"/{resource_name}"
            )
            self._convert_methods(resource_data.get("methods", {}))
            self._convert_resources(
                resource_data.get("resources", {}), resource_path
            )

    def _convert_methods(self, methods: dict[str, Any]) -> None:
        for method_data in methods.values():
            http_method = method_data.get("httpMethod", "GET").lower()
            rest_path = method_data.get(
                "flatPath", method_data.get("path", "/")
            )
            if not rest_path.startswith("/"):
                rest_path = "/" + rest_path

            path_params = DiscoveryConverter._extract_path_parameters(rest_path)

            if rest_path not in self._openapi_spec["paths"]:
                self._openapi_spec["paths"][rest_path] = {}

            self._openapi_spec["paths"][rest_path][http_method] = (
                self._convert_operation(method_data, path_params)
            )

    @staticmethod
    def _extract_path_parameters(path: str) -> list[str]:

        return [
            match.replace("+", "") for match in re.findall(r"\{([^}]+)\}", path)
        ]

    def _convert_operation(
        self, method_data: dict[str, Any], path_params: list[str]
    ) -> dict[str, Any]:

        op_id = method_data.get("id", "").replace(".", "_")
        op_id = re.sub(r"(?<!^)(?=[A-Z])", "_", op_id).lower()

        operation = {
            "operationId": op_id,
            "summary": method_data.get("description", "").split(". List")[0],
            "description": method_data.get("description", ""),
            "parameters": [],
            "responses": {"200": {"description": "Successful operation"}},
        }

        for param_name in path_params:
            operation["parameters"].append({
                "name": param_name,
                "in": "path",
                "required": True,
                "schema": {"type": "string"},
            })

        for param_name, param_data in method_data.get("parameters", {}).items():
            if param_name in path_params:
                continue  # pragma: no cover
            operation["parameters"].append({
                "name": param_name,
                "in": param_data.get("location", "query"),
                "description": param_data.get("description", ""),
                "required": param_data.get("required", False),
                "schema": self._convert_schema_object(param_data),
            })

        if "request" in method_data:
            req_ref = method_data["request"].get("$ref", "")
            if req_ref:
                ref_path = (
                    req_ref.replace("#", "#/components/schemas/")
                    if req_ref.startswith("#")
                    else f"#/components/schemas/{req_ref}"
                )
                operation["requestBody"] = {
                    "description": "Request body",
                    "content": {
                        "application/json": {"schema": {"$ref": ref_path}}
                    },
                    "required": True,
                }

        return operation


if __name__ == "__main__":  # pragma: no cover
    from adspace_agent.tools.google_ads.toolset import GOOGLE_ADS_API_VERSION

    converter = DiscoveryConverter(
        f"https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}"
    )
    spec = converter.convert()
    print(
        f"Generated {len(spec['paths'])} paths with {len(spec['components']['schemas'])} schemas."
    )
