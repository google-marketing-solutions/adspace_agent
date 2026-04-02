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
import json
import pathlib
import tempfile
from unittest import mock

from adspace_agent.tools.google_ads.discovery_converter import (
    DiscoveryConverter,
)


def test_discovery_converter():
    discovery_data = {
        "title": "Google Ads API",
        "description": "Test API",
        "version": "v1",
        "rootUrl": "https://example.com/",
        "servicePath": "v1/",
        "schemas": {
            "TestSchema": {
                "type": "object",
                "properties": {
                    "prop1": {"type": "string"},
                    "reqProp": {"type": "integer", "required": True},
                },
            },
            "ArraySchema": {"type": "array", "items": {"type": "string"}},
            "AnySchema": {"type": "any"},
            "RefSchema": {
                "$ref": "TestSchema",
                "format": "date",
                "enum": ["a"],
                "description": "desc",
                "pattern": ".*",
                "default": "def",
            },
            "HashRefSchema": {"$ref": "#TestSchema"},
        },
        "resources": {
            "customers": {
                "methods": {
                    "search": {
                        "id": "customers.search",
                        "description": "Search customers. List them.",
                        "httpMethod": "POST",
                        "flatPath": "v1/customers/{customerId}:search",
                        "parameters": {
                            "customerId": {
                                "type": "string",
                                "location": "path",
                            },
                            "queryParam": {
                                "type": "string",
                                "location": "query",
                            },
                        },
                        "request": {"$ref": "TestSchema"},
                    }
                },
                "resources": {
                    "nested": {
                        "methods": {
                            "get": {
                                "id": "customers.nested.get",
                                "path": "/v1/customers/nested",
                                "request": {"$ref": "#RefSchema"},
                            }
                        }
                    }
                },
            }
        },
    }

    with tempfile.NamedTemporaryFile("w", delete=False, encoding="utf-8") as f:
        json.dump(discovery_data, f)
        temp_path = f.name

    try:
        converter = DiscoveryConverter(temp_path)
        spec = converter.convert()

        assert spec["info"]["title"] == "Google Ads API"
        assert spec["servers"][0]["url"] == "https://example.com/v1"
        assert "TestSchema" in spec["components"]["schemas"]
        assert "/v1/customers/{customerId}:search" in spec["paths"]
    finally:
        pathlib.Path(temp_path).unlink()


def test_discovery_converter_url():
    discovery_data = {
        "title": "Google Ads API",
        "description": "Test API",
        "version": "v1",
        "rootUrl": "https://example.com/",
        "servicePath": "v1/",
        "schemas": {
            "TestSchema": {
                "type": "object",
                "properties": {
                    "prop1": {"type": "string"},
                    "reqProp": {"type": "integer", "required": True},
                },
            },
            "ArraySchema": {"type": "array", "items": {"type": "string"}},
            "AnySchema": {"type": "any"},
            "RefSchema": {
                "$ref": "TestSchema",
                "format": "date",
                "enum": ["a"],
                "description": "desc",
                "pattern": ".*",
                "default": "def",
            },
            "HashRefSchema": {"$ref": "#TestSchema"},
        },
        "resources": {
            "customers": {
                "methods": {
                    "search": {
                        "id": "customers.search",
                        "description": "Search customers. List them.",
                        "httpMethod": "POST",
                        "flatPath": "v1/customers/{customerId}:search",
                        "parameters": {
                            "customerId": {
                                "type": "string",
                                "location": "path",
                            },
                            "queryParam": {
                                "type": "string",
                                "location": "query",
                            },
                        },
                        "request": {"$ref": "TestSchema"},
                    }
                },
                "resources": {
                    "nested": {
                        "methods": {
                            "get": {
                                "id": "customers.nested.get",
                                "path": "/v1/customers/nested",
                                "request": {"$ref": "#RefSchema"},
                            }
                        }
                    }
                },
            }
        },
    }
    with mock.patch("httpx.get") as mock_get:
        mock_resp = mock.Mock()
        mock_resp.json.return_value = discovery_data
        mock_get.return_value = mock_resp

        converter = DiscoveryConverter("https://fake.url")
        spec = converter.convert()

        assert spec["info"]["title"] == "Google Ads API"
        mock_get.assert_called_once_with("https://fake.url", timeout=30.0)
