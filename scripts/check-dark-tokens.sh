#!/usr/bin/env bash
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

# Verifies that the two hand-mirrored dark-theme token blocks in
# src/studio.css have not drifted apart:
#
#   @media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ... } }
#   :root[data-theme="dark"] { ... }
#
# There is no preprocessor (the file is served verbatim by the Rust server),
# so the values are duplicated on purpose. This script extracts both blocks,
# strips indentation, and diffs them. Exits non-zero on drift.
#
# Usage: scripts/check-dark-tokens.sh [path/to/studio.css]

set -euo pipefail

css="${1:-$(cd "$(dirname "$0")/.." && pwd)/src/studio.css}"

if [[ ! -f "$css" ]]; then
  echo "check-dark-tokens: $css not found" >&2
  exit 2
fi

# Block A: body of `:root:not([data-theme="light"]) {` inside the media query,
# up to its closing brace (the first line that is exactly two spaces + `}`).
extract_media() {
  awk '
    /^  :root:not\(\[data-theme="light"\]\) \{/ { grab = 1; next }
    grab && /^  \}/ { exit }
    grab { print }
  ' "$css"
}

# Block B: body of top-level `:root[data-theme="dark"] {` up to its `}`.
extract_explicit() {
  awk '
    /^:root\[data-theme="dark"\] \{/ { grab = 1; next }
    grab && /^\}/ { exit }
    grab { print }
  ' "$css"
}

normalise() {
  sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' | grep -v '^$'
}

a="$(extract_media | normalise)"
b="$(extract_explicit | normalise)"

if [[ -z "$a" || -z "$b" ]]; then
  echo "check-dark-tokens: could not locate both dark token blocks in $css" >&2
  exit 2
fi

if diff_out="$(diff <(printf '%s\n' "$a") <(printf '%s\n' "$b"))"; then
  echo "check-dark-tokens: OK ($(printf '%s\n' "$a" | wc -l | tr -d ' ') lines match)"
else
  echo "check-dark-tokens: dark token blocks have drifted (< media query, > [data-theme=dark]):" >&2
  printf '%s\n' "$diff_out" >&2
  exit 1
fi
