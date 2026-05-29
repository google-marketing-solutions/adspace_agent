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
"""Temporary patch for AuthHandler to support Reasoning Engine oauth tokens."""

import os
import typing

from google.adk.auth.auth_credential import AuthCredential
from google.adk.auth.auth_credential import AuthCredentialTypes
from google.adk.auth.auth_credential import OAuth2Auth
from google.adk.auth.auth_handler import AuthHandler
from google.adk.sessions.state import State

AUTH_KEY: str = os.environ.get("ADSPACE_AGENT_AUTH_KEY", "adspace_agent")


def patch_auth_handler() -> None:
    """Patches AuthHandler to support Reasoning Engine oauth tokens."""
    original_get_auth_response = AuthHandler.get_auth_response

    def patched_get_auth_response(
        self: AuthHandler, state: State
    ) -> AuthCredential | None:
        res = original_get_auth_response(self, state)
        if res is not None:
            return res

        def _build_cred(token: str) -> AuthCredential:
            oauth2_auth = OAuth2Auth(access_token=token)
            return AuthCredential(
                auth_type=AuthCredentialTypes.OAUTH2,
                oauth2=oauth2_auth,
            )

        # 1. Check self.auth_config.credential_key first
        key = self.auth_config.credential_key
        if key:
            val = state.get(key)
            if isinstance(val, str) and val:
                return _build_cred(val)

        # 2. Check global connection AUTH_KEY
        val = state.get(AUTH_KEY)
        if isinstance(val, str) and val:
            return _build_cred(val)

        # 3. Scan state for any active OAuth access token
        try:
            for k in state:
                v = state.get(k)
                if isinstance(v, str) and v.startswith("ya29."):
                    return _build_cred(v)
        except Exception:  # noqa: S110, BLE001
            pass

        return None

    typing.cast(
        "typing.Any", AuthHandler
    ).get_auth_response = patched_get_auth_response


patch_auth_handler()
