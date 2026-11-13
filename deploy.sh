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

# Builds the container image with cloudbuild.yaml (fast: 32-vCPU builder with a
# registry layer cache) and rolls it out to Cloud Run.
#
# Existing environment variables and secrets of the service are preserved; this
# script never sets them. Configure them once with `gcloud run deploy
# --set-env-vars/--set-secrets` (see README "Deploy to Cloud Run") or pass extra
# gcloud flags after `--`:
#
#   ./deploy.sh                                  # build + deploy
#   ./deploy.sh --skip-build                     # redeploy the current :latest image
#   TAG=897b08342f62 ./deploy.sh --skip-build    # deploy an earlier build again (roll back)
#   ./deploy.sh -- --update-env-vars ADMIN_EMAILS=you@example.com
#
# Each build is pushed as :latest and under a tag naming what was built: the
# commit (`git rev-parse --short=12 HEAD`), with `-dirty-<time>` appended when
# the working tree has uncommitted changes, or `build-<time>` outside a git
# checkout. The deploy uses that tag, so a revision names the build it runs and
# any earlier build can be deployed again by its tag.
#
# Settings (environment variables, all optional):
#   PROJECT_ID  Google Cloud project   (default: gcloud config project)
#   REGION      Cloud Run / Cloud Build region   (default: us-central1)
#   SERVICE     Cloud Run service name           (default: adspace-agent-rs)
#   AR_REPO     Artifact Registry repository     (default: cloud-run-source-deploy)
#   TAG         Image tag to build and deploy    (default: the commit, as above;
#               with --skip-build: latest)
#
# REGION, SERVICE and AR_REPO must match the substitutions in cloudbuild.yaml.
set -euo pipefail

cd "$(dirname "$0")"

PROJECT_ID="${PROJECT_ID:-$(gcloud config get-value project 2>/dev/null)}"
REGION="${REGION:-us-central1}"
SERVICE="${SERVICE:-adspace-agent-rs}"
AR_REPO="${AR_REPO:-cloud-run-source-deploy}"

skip_build=false
extra_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) skip_build=true; shift ;;
    --) shift; extra_args=("$@"); break ;;
    -h|--help) sed -n '16,36p' "$0"; exit 0 ;;
    *) echo "Unknown argument: $1 (pass gcloud flags after --)" >&2; exit 2 ;;
  esac
done

if [[ -z "$PROJECT_ID" ]]; then
  echo "PROJECT_ID is not set and gcloud has no default project." >&2
  exit 2
fi

# The tag naming what is about to be built.
build_tag() {
  local stamp commit
  stamp="$(date -u +%Y%m%d%H%M%S)"
  if commit="$(git rev-parse --short=12 HEAD 2>/dev/null)"; then
    if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
      echo "Note: the working tree has uncommitted changes; they will be built." >&2
      echo "${commit}-dirty-${stamp}"
    else
      echo "$commit"
    fi
  else
    echo "build-${stamp}"
  fi
}

if [[ "$skip_build" == true ]]; then
  TAG="${TAG:-latest}"
else
  TAG="${TAG:-$(build_tag)}"
fi
if [[ ! "$TAG" =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$ ]]; then
  echo "TAG '${TAG}' is not a valid image tag." >&2
  exit 2
fi

IMAGE="${REGION}-docker.pkg.dev/${PROJECT_ID}/${AR_REPO}/${SERVICE}:${TAG}"

if [[ "$skip_build" == false ]]; then
  echo "==> Building ${IMAGE} with cloudbuild.yaml"
  gcloud builds submit \
    --project "$PROJECT_ID" \
    --region "$REGION" \
    --config cloudbuild.yaml \
    --substitutions "_REGION=${REGION},_AR_REPO=${AR_REPO},_SERVICE=${SERVICE},_TAG=${TAG}" \
    .
fi

echo "==> Deploying ${IMAGE} to Cloud Run service ${SERVICE} (${REGION})"
gcloud run deploy "$SERVICE" \
  --project "$PROJECT_ID" \
  --region "$REGION" \
  --image "$IMAGE" \
  --quiet \
  ${extra_args[@]+"${extra_args[@]}"}

echo "==> Live:"
gcloud run services describe "$SERVICE" \
  --project "$PROJECT_ID" \
  --region "$REGION" \
  --format='value(status.url,status.latestReadyRevisionName)'
