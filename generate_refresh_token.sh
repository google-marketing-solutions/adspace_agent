#!/bin/bash

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

set -e

url_decode() {
  local url_encoded="${1//+/ }"
  printf '%b' "${url_encoded//%/\\x}"
}

read -r -p "Enter your Google Ads Client ID: " CLIENT_ID
read -r -s -p "Enter your Google Ads Client Secret: " CLIENT_SECRET
echo
echo
echo "Please visit the following URL in your browser to authorize:"
echo
echo "https://accounts.google.com/o/oauth2/auth?client_id=${CLIENT_ID}&redirect_uri=http://localhost&scope=https://www.googleapis.com/auth/adwords&response_type=code&access_type=offline"
echo

read -r -p "Paste the entire URL from the redirect so the 'code' parameter can be extracted: " AUTH_URL

AUTH_URL=$(url_decode "$AUTH_URL")
AUTH_CODE=$(echo "$AUTH_URL" | grep -o 'code=[^&]*' | cut -d'=' -f2)

echo
echo "Requesting refresh token..."
TOKEN_RESPONSE=$(curl --silent --request POST \
  --data "code=${AUTH_CODE}" \
  --data "client_id=${CLIENT_ID}" \
  --data "client_secret=${CLIENT_SECRET}" \
  --data "redirect_uri=http://localhost" \
  --data "grant_type=authorization_code" \
  https://oauth2.googleapis.com/token)

TOKEN_PART="${TOKEN_RESPONSE#*\"refresh_token\": \"}"
REFRESH_TOKEN="${TOKEN_PART%%\"*}"

if [ -z "$REFRESH_TOKEN" ]; then
  echo "Error: Could not obtain refresh token. Please double-check your credentials and the authorization redirect URL."
  echo "Google's Response: ${TOKEN_RESPONSE}"
  exit 1
fi

echo "Success! The refresh token can now be added to your .env file:"
echo
echo "export GOOGLE_ADS_REFRESH_TOKEN=${REFRESH_TOKEN}"
echo
