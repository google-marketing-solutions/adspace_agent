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

# Verifies that the two hand-mirrored dark-theme token blocks in each
# stylesheet (src/studio.css and src/adk-ui.css) have not drifted apart:
#
#   @media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { ... } }
#   :root[data-theme="dark"] { ... }
#
# There is no preprocessor (the files are served verbatim by the Rust server),
# so the values are duplicated on purpose. This script extracts both blocks,
# strips indentation, and diffs them. Exits non-zero on drift.
#
# Usage: scripts/check-dark-tokens.sh [path/to/file.css ...]

set -euo pipefail

if [[ $# -gt 0 ]]; then
  files=("$@")
else
  src="$(cd "$(dirname "$0")/.." && pwd)/src"
  files=("$src/studio.css" "$src/adk-ui.css")
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

status=0
for css in "${files[@]}"; do
  if [[ ! -f "$css" ]]; then
    echo "check-dark-tokens: $css not found" >&2
    exit 2
  fi

  a="$(extract_media | normalise)"
  b="$(extract_explicit | normalise)"

  if [[ -z "$a" || -z "$b" ]]; then
    echo "check-dark-tokens: could not locate both dark token blocks in $css" >&2
    exit 2
  fi

  if diff_out="$(diff <(printf '%s\n' "$a") <(printf '%s\n' "$b"))"; then
    echo "check-dark-tokens: OK $(basename "$css") ($(printf '%s\n' "$a" | wc -l | tr -d ' ') lines match)"
  else
    echo "check-dark-tokens: dark token blocks in $css have drifted (< media query, > [data-theme=dark]):" >&2
    printf '%s\n' "$diff_out" >&2
    status=1
  fi
done
exit "$status"
