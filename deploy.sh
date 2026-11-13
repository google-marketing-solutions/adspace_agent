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
# registry layer cache) and rolls it out to Cloud Run and/or Gemini Enterprise
# (Vertex AI Agent Runtime ReasoningEngine BYOC + Discovery Engine registration).
#
# Usage:
#   ./deploy.sh                                  # build + deploy to Cloud Run
#   ./deploy.sh --skip-build                     # redeploy the current :latest image
#   TAG=897b08342f62 ./deploy.sh --skip-build    # deploy an earlier build again (roll back)
#   ./deploy.sh -- --update-env-vars ADMIN_EMAILS=you@example.com
#   ./deploy.sh --gemini-enterprise --app-id my-ge-app
#   ./deploy.sh --gemini-enterprise --app-id my-ge-app --auth-id adspace-oauth
#   ./deploy.sh --target all --app-id my-ge-app
#
# Each build is pushed as :latest and under a tag naming what was built: the
# commit (`git rev-parse --short=12 HEAD`), with `-dirty-<time>` appended when
# the working tree has uncommitted changes, or `build-<time>` outside a git
# checkout. The deploy uses that tag, so a revision names the build it runs and
# any earlier build can be deployed again by its tag.
#
# Settings (environment variables, all optional):
#   PROJECT_ID                  Google Cloud project (default: gcloud config project)
#   REGION                      Cloud Run / Agent Runtime region (default: us-central1)
#   SERVICE                     Cloud Run service name (default: adspace-agent-rs)
#   AR_REPO                     Artifact Registry repository (default: cloud-run-source-deploy)
#   TAG                         Image tag to build and deploy (default: commit; with --skip-build: latest)
#   DEPLOY_TARGET               Deployment target: cloud-run, gemini-enterprise, or all (default: cloud-run)
#   GEMINI_ENTERPRISE_APP_ID    Gemini Enterprise app/engine ID to register the ADK agent on
#   GEMINI_ENTERPRISE_LOCATION  Gemini Enterprise multi-region: global, us, or eu (default: global)
#   GEMINI_ENTERPRISE_AUTH_ID   Optional Gemini Enterprise OAuth authorization ID (toolAuthorizations)
#   GEMINI_ENTERPRISE_AGENT_ID  Optional existing Gemini Enterprise agent ID to update in-place
#   REASONING_ENGINE_ID         Optional existing Vertex AI ReasoningEngine ID to update in-place
#   AGENT_DISPLAY_NAME          Display name in Agent Runtime & Gemini Enterprise (default: AdSpace Agent)
#   AGENT_DESCRIPTION           Agent description in Agent Runtime & Gemini Enterprise
#   AGENT_ICON_URI              Optional public icon URI for Gemini Enterprise
#   OAUTH_SCOPES                Space-separated OAuth scopes for GEMINI_ENTERPRISE_AUTH_ID
#
# REGION, SERVICE and AR_REPO must match the substitutions in cloudbuild.yaml.
set -euo pipefail

cd "$(dirname "$0")"

PROJECT_ID="${PROJECT_ID:-$(gcloud config get-value project 2>/dev/null)}"
REGION="${REGION:-us-central1}"
SERVICE="${SERVICE:-adspace-agent-rs}"
AR_REPO="${AR_REPO:-cloud-run-source-deploy}"
DEPLOY_TARGET="${DEPLOY_TARGET:-cloud-run}"
GEMINI_ENTERPRISE_LOCATION="${GEMINI_ENTERPRISE_LOCATION:-global}"
GEMINI_ENTERPRISE_APP_ID="${GEMINI_ENTERPRISE_APP_ID:-}"
GEMINI_ENTERPRISE_AUTH_ID="${GEMINI_ENTERPRISE_AUTH_ID:-}"
GEMINI_ENTERPRISE_AGENT_ID="${GEMINI_ENTERPRISE_AGENT_ID:-}"
REASONING_ENGINE_ID="${REASONING_ENGINE_ID:-}"
AGENT_DISPLAY_NAME="${AGENT_DISPLAY_NAME:-AdSpace Agent}"
AGENT_DESCRIPTION="${AGENT_DESCRIPTION:-AdSpace Agent is designed to provide a standardized way to integrate an LLM with Google Ads, Display & Video 360, Campaign Manager 360, Search Ads 360, YouTube, Google Drive, BigQuery, and Google Cloud Storage to form a comprehensive campaign and marketing plan for agencies.}"
AGENT_ICON_URI="${AGENT_ICON_URI:-}"
DEFAULT_OAUTH_SCOPES="https://www.googleapis.com/auth/adwords https://www.googleapis.com/auth/dfareporting https://www.googleapis.com/auth/dfatrafficking https://www.googleapis.com/auth/ddmconversions https://www.googleapis.com/auth/display-video https://www.googleapis.com/auth/doubleclickbidmanager https://www.googleapis.com/auth/doubleclicksearch https://www.googleapis.com/auth/bigquery https://www.googleapis.com/auth/drive https://www.googleapis.com/auth/devstorage.full_control https://www.googleapis.com/auth/youtube.readonly https://www.googleapis.com/auth/content"
OAUTH_SCOPES="${OAUTH_SCOPES:-$DEFAULT_OAUTH_SCOPES}"

skip_build=false
extra_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) skip_build=true; shift ;;
    --gemini-enterprise) DEPLOY_TARGET="gemini-enterprise"; shift ;;
    --target)
      if [[ $# -lt 2 ]]; then echo "--target requires a value (cloud-run, gemini-enterprise, or all)" >&2; exit 2; fi
      DEPLOY_TARGET="$2"; shift 2 ;;
    --target=*) DEPLOY_TARGET="${1#*=}"; shift ;;
    --app-id)
      if [[ $# -lt 2 ]]; then echo "--app-id requires a Gemini Enterprise app ID" >&2; exit 2; fi
      GEMINI_ENTERPRISE_APP_ID="$2"; shift 2 ;;
    --app-id=*) GEMINI_ENTERPRISE_APP_ID="${1#*=}"; shift ;;
    --auth-id)
      if [[ $# -lt 2 ]]; then echo "--auth-id requires an authorization ID" >&2; exit 2; fi
      GEMINI_ENTERPRISE_AUTH_ID="$2"; shift 2 ;;
    --auth-id=*) GEMINI_ENTERPRISE_AUTH_ID="${1#*=}"; shift ;;
    --engine-id)
      if [[ $# -lt 2 ]]; then echo "--engine-id requires a ReasoningEngine ID" >&2; exit 2; fi
      REASONING_ENGINE_ID="$2"; shift 2 ;;
    --engine-id=*) REASONING_ENGINE_ID="${1#*=}"; shift ;;
    --agent-id)
      if [[ $# -lt 2 ]]; then echo "--agent-id requires a Gemini Enterprise agent ID" >&2; exit 2; fi
      GEMINI_ENTERPRISE_AGENT_ID="$2"; shift 2 ;;
    --agent-id=*) GEMINI_ENTERPRISE_AGENT_ID="${1#*=}"; shift ;;
    --ge-location)
      if [[ $# -lt 2 ]]; then echo "--ge-location requires global, us, or eu" >&2; exit 2; fi
      GEMINI_ENTERPRISE_LOCATION="$2"; shift 2 ;;
    --ge-location=*) GEMINI_ENTERPRISE_LOCATION="${1#*=}"; shift ;;
    --) shift; extra_args=("$@"); break ;;
    -h|--help) sed -n '16,52p' "$0"; exit 0 ;;
    *) echo "Unknown argument: $1 (pass gcloud flags after --)" >&2; exit 2 ;;
  esac
done

case "$DEPLOY_TARGET" in
  cloud-run|gemini-enterprise|all) ;;
  *)
    echo "Invalid DEPLOY_TARGET '${DEPLOY_TARGET}'; expected cloud-run, gemini-enterprise, or all." >&2
    exit 2
    ;;
esac

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

deploy_cloud_run() {
  echo "==> Deploying ${IMAGE} to Cloud Run service ${SERVICE} (${REGION})"
  gcloud run deploy "$SERVICE" \
    --project "$PROJECT_ID" \
    --region "$REGION" \
    --image "$IMAGE" \
    --quiet \
    ${extra_args[@]+"${extra_args[@]}"}

  echo "==> Live (Cloud Run):"
  gcloud run services describe "$SERVICE" \
    --project "$PROJECT_ID" \
    --region "$REGION" \
    --format='value(status.url,status.latestReadyRevisionName)'
}

gcp_access_token() {
  local tok="" attempt
  for attempt in 1 2 3; do
    if tok="$(gcloud auth application-default print-access-token 2>/dev/null)" && [[ -n "$tok" ]]; then
      printf '%s\n' "$tok"
      return 0
    fi
    sleep 1
  done
  gcloud auth print-access-token
}

# Deploys/updates the Vertex AI Agent Runtime (ReasoningEngine BYOC) resource
# and optionally registers/updates the ADK agent in Gemini Enterprise.
deploy_gemini_enterprise() {
  case "$GEMINI_ENTERPRISE_LOCATION" in
    global) ;;
    us)
      if [[ "$REGION" != us-* ]]; then
        echo "GEMINI_ENTERPRISE_LOCATION=us requires a US region (us-*), got REGION=${REGION}." >&2
        exit 2
      fi
      ;;
    eu)
      if [[ "$REGION" != europe-* ]]; then
        echo "GEMINI_ENTERPRISE_LOCATION=eu requires an EU region (europe-*), got REGION=${REGION}." >&2
        exit 2
      fi
      ;;
    *)
      echo "Invalid GEMINI_ENTERPRISE_LOCATION '${GEMINI_ENTERPRISE_LOCATION}'; expected global, us, or eu." >&2
      exit 2
      ;;
  esac

  local token aiplatform_base discovery_base project_number run_spec_json
  token="$(gcp_access_token)"
  aiplatform_base="https://${REGION}-aiplatform.googleapis.com/v1beta1"
  if [[ "$GEMINI_ENTERPRISE_LOCATION" == "global" ]]; then
    discovery_base="https://discoveryengine.googleapis.com/v1alpha"
  else
    discovery_base="https://${GEMINI_ENTERPRISE_LOCATION}-discoveryengine.googleapis.com/v1alpha"
  fi
  project_number="$(gcloud projects describe "$PROJECT_ID" --format='value(projectNumber)' 2>/dev/null || echo "$PROJECT_ID")"

  # If GEMINI_ENTERPRISE_APP_ID is set, inspect existing agents on the app first
  # so we can reuse its ReasoningEngine ID and preserve existing toolAuthorizations.
  local agents_parent="" existing_agent_name="" existing_agent_engine="" existing_agent_auth=""
  if [[ -n "$GEMINI_ENTERPRISE_APP_ID" ]]; then
    agents_parent="projects/${PROJECT_ID}/locations/${GEMINI_ENTERPRISE_LOCATION}/collections/default_collection/engines/${GEMINI_ENTERPRISE_APP_ID}/assistants/default_assistant/agents"
    local pre_agents_resp
    pre_agents_resp="$(curl -sS \
      -H "Authorization: Bearer ${token}" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${discovery_base}/${agents_parent}" 2>/dev/null || echo "{}")"
    eval "$(python3 -c '
import json, shlex, sys
doc = json.loads(sys.argv[1])
target_agent_id, target_engine_id, display_name = sys.argv[2:5]
matched = None
for agent in doc.get("agents") or []:
    name = agent.get("name", "")
    re_name = (
        agent.get("adkAgentDefinition", {})
        .get("provisionedReasoningEngine", {})
        .get("reasoningEngine", "")
    )
    if target_agent_id and (name == target_agent_id or name.endswith("/" + target_agent_id)):
        matched = agent
        break
    if target_engine_id and (re_name == target_engine_id or re_name.endswith("/" + target_engine_id)):
        matched = agent
        break
    if agent.get("displayName") == display_name and "adkAgentDefinition" in agent:
        matched = agent
        break
if matched:
    re_name = (
        matched.get("adkAgentDefinition", {})
        .get("provisionedReasoningEngine", {})
        .get("reasoningEngine", "")
    )
    auths = matched.get("authorizationConfig", {}).get("toolAuthorizations") or []
    first_auth = auths[0] if auths else ""
    print("existing_agent_name=" + shlex.quote(matched.get("name", "")))
    print("existing_agent_engine=" + shlex.quote(re_name))
    print("existing_agent_auth=" + shlex.quote(first_auth))
' "$pre_agents_resp" "$GEMINI_ENTERPRISE_AGENT_ID" "$REASONING_ENGINE_ID" "$AGENT_DISPLAY_NAME")"
  fi

  local effective_auth_id="$GEMINI_ENTERPRISE_AUTH_ID"
  if [[ -z "$effective_auth_id" && -n "$existing_agent_auth" ]]; then
    effective_auth_id="${existing_agent_auth##*/}"
  fi

  # Inspect the existing Cloud Run service (if present) so Agent Runtime
  # inherits the same service account, environment variables, and Secret Manager
  # bindings without duplicating configuration.
  run_spec_json="$(gcloud run services describe "$SERVICE" \
    --project "$PROJECT_ID" \
    --region "$REGION" \
    --format=json 2>/dev/null || echo "{}")"

  local resolved_sa
  resolved_sa="${SERVICE_ACCOUNT:-$(python3 -c 'import json,sys; print(json.loads(sys.argv[1]).get("spec",{}).get("template",{}).get("spec",{}).get("serviceAccountName",""))' "$run_spec_json" 2>/dev/null || true)}"
  if [[ -n "$resolved_sa" ]]; then
    gcloud projects add-iam-policy-binding "$PROJECT_ID" \
      --member="serviceAccount:${resolved_sa}" \
      --role="roles/serviceusage.serviceUsageConsumer" \
      --quiet >/dev/null 2>&1 || true
  fi

  local engine_payload
  engine_payload="$(python3 -c '
import json, os, sys

run_doc = json.loads(sys.argv[1])
image_uri = sys.argv[2]
display_name = sys.argv[3]
description = sys.argv[4]
auth_id = sys.argv[5]
project_id = sys.argv[6]

spec = run_doc.get("spec", {}).get("template", {}).get("spec", {})
service_account = os.environ.get("SERVICE_ACCOUNT") or spec.get("serviceAccountName") or ""
containers = spec.get("containers") or [{}]
raw_env = containers[0].get("env") or []

# Agent Engine reserves these names and sets them automatically.
reserved = {
    "PORT",
    "K_SERVICE",
    "K_REVISION",
    "K_CONFIGURATION",
    "GOOGLE_CLOUD_PROJECT",
    "GOOGLE_CLOUD_LOCATION",
    "GOOGLE_CLOUD_AGENT_ENGINE_ID",
}

env_map = {}
secret_env = []
model_location = "global"

for item in raw_env:
    name = item.get("name", "")
    if not name:
        continue
    if name == "GOOGLE_CLOUD_LOCATION" and item.get("value"):
        model_location = item["value"]
        continue
    if name == "GOOGLE_CLOUD_PROJECT" and item.get("value"):
        project_id = item["value"]
        continue
    if name in reserved or name.startswith("GOOGLE_CLOUD_AGENT_ENGINE"):
        continue
    if "value" in item:
        env_map[name] = item["value"]
    elif "valueFrom" in item:
        ref = item["valueFrom"].get("secretKeyRef") or {}
        secret_name = ref.get("name")
        version = ref.get("key") or "latest"
        if secret_name:
            secret_env.append({
                "name": name,
                "secretRef": {
                    "secret": secret_name,
                    "version": version,
                },
            })

env_map["AGENT_ENGINE"] = "true"
env_map.setdefault("GOOGLE_GENAI_USE_VERTEXAI", "TRUE")
env_map.setdefault("GOOGLE_MODEL_LOCATION", model_location)
if project_id:
    env_map.setdefault("GOOGLE_PROJECT_ID", project_id)
if auth_id:
    env_map["GEMINI_ENTERPRISE_AUTH_ID"] = auth_id

env_list = [{"name": k, "value": v} for k, v in sorted(env_map.items())]

session_id_params = {
    "type": "object",
    "properties": {
        "user_id": {"type": "string"},
        "session_id": {"type": "string"},
    },
    "required": ["user_id", "session_id"],
}
user_id_params = {
    "type": "object",
    "properties": {
        "user_id": {"type": "string"},
    },
    "required": ["user_id"],
}
create_session_params = {
    "type": "object",
    "properties": {
        "user_id": {"type": "string"},
        "session_id": {"type": "string", "nullable": True},
        "state": {"type": "object", "nullable": True},
    },
    "required": ["user_id"],
}
stream_query_params = {
    "type": "object",
    "properties": {
        "message": {
            "anyOf": [
                {"type": "string"},
                {"type": "object", "additionalProperties": True},
            ]
        },
        "user_id": {"type": "string"},
        "session_id": {"type": "string", "nullable": True},
        "run_config": {"type": "object", "nullable": True},
    },
    "required": ["message", "user_id"],
}
async_stream_query_params = {
    "type": "object",
    "properties": {
        "message": {
            "anyOf": [
                {"type": "string"},
                {"type": "object", "additionalProperties": True},
            ]
        },
        "user_id": {"type": "string"},
        "session_id": {"type": "string", "nullable": True},
        "session_events": {"type": "array", "nullable": True},
        "run_config": {"type": "object", "nullable": True},
    },
    "required": ["message", "user_id"],
}

class_methods = [
    {"name": "get_session", "api_mode": "", "parameters": session_id_params},
    {"name": "list_sessions", "api_mode": "", "parameters": user_id_params},
    {"name": "create_session", "api_mode": "", "parameters": create_session_params},
    {"name": "delete_session", "api_mode": "", "parameters": session_id_params},
    {"name": "register_operations", "api_mode": "", "parameters": {"type": "object", "properties": {}}},
    {"name": "async_get_session", "api_mode": "async", "parameters": session_id_params},
    {"name": "async_list_sessions", "api_mode": "async", "parameters": user_id_params},
    {"name": "async_create_session", "api_mode": "async", "parameters": create_session_params},
    {"name": "async_delete_session", "api_mode": "async", "parameters": session_id_params},
    {
        "name": "async_add_session_to_memory",
        "api_mode": "async",
        "parameters": {
            "type": "object",
            "properties": {"session": {"type": "object", "additionalProperties": True}},
            "required": ["session"],
        },
    },
    {
        "name": "async_search_memory",
        "api_mode": "async",
        "parameters": {
            "type": "object",
            "properties": {"user_id": {"type": "string"}, "query": {"type": "string"}},
            "required": ["user_id", "query"],
        },
    },
    {"name": "stream_query", "api_mode": "stream", "parameters": stream_query_params},
    {"name": "async_stream_query", "api_mode": "async_stream", "parameters": async_stream_query_params},
    {
        "name": "streaming_agent_run_with_events",
        "api_mode": "async_stream",
        "parameters": {
            "type": "object",
            "properties": {"request_json": {"type": "string"}},
            "required": ["request_json"],
        },
    },
]

deployment_spec = {
    "env": env_list,
    "resourceLimits": {
        "cpu": "4",
        "memory": "4Gi",
    },
}
if secret_env:
    deployment_spec["secretEnv"] = secret_env

engine_spec = {
    "agentFramework": "google-adk",
    "containerSpec": {
        "imageUri": image_uri,
        "port": 8080,
    },
    "deploymentSpec": deployment_spec,
    "classMethods": class_methods,
}
if service_account:
    engine_spec["serviceAccount"] = service_account

payload = {
    "displayName": display_name,
    "description": description,
    "spec": engine_spec,
}
print(json.dumps(payload))
' "$run_spec_json" "$IMAGE" "$AGENT_DISPLAY_NAME" "$AGENT_DESCRIPTION" "$effective_auth_id" "$PROJECT_ID")"

  local existing_engine=""
  if [[ -n "$REASONING_ENGINE_ID" ]]; then
    if [[ "$REASONING_ENGINE_ID" == projects/* ]]; then
      existing_engine="$REASONING_ENGINE_ID"
    else
      existing_engine="projects/${PROJECT_ID}/locations/${REGION}/reasoningEngines/${REASONING_ENGINE_ID}"
    fi
  elif [[ -n "$existing_agent_engine" ]]; then
    existing_engine="$existing_agent_engine"
  else
    local list_resp
    list_resp="$(curl -sS \
      -H "Authorization: Bearer ${token}" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${aiplatform_base}/projects/${PROJECT_ID}/locations/${REGION}/reasoningEngines" 2>/dev/null || echo "{}")"
    existing_engine="$(python3 -c '
import json, sys
doc = json.loads(sys.argv[1])
target = sys.argv[2]
for item in doc.get("reasoningEngines") or []:
    if item.get("displayName") == target and item.get("name"):
        print(item["name"])
        break
' "$list_resp" "$AGENT_DISPLAY_NAME")"
  fi

  local op_resp op_name
  if [[ -n "$existing_engine" ]]; then
    echo "==> Updating existing Agent Runtime ReasoningEngine ${existing_engine} with ${IMAGE}"
    op_resp="$(curl -sS -X PATCH \
      -H "Authorization: Bearer ${token}" \
      -H "Content-Type: application/json" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${aiplatform_base}/${existing_engine}?updateMask=displayName,description,spec" \
      -d "$engine_payload")"
    op_name="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1]).get("name", ""))' "$op_resp" 2>/dev/null || true)"
    if [[ -z "$op_name" && -z "$REASONING_ENGINE_ID" ]]; then
      echo "Note: In-place update of ${existing_engine} was rejected (${op_resp}); creating a new container-based ReasoningEngine instead..." >&2
      existing_engine=""
    fi
  fi

  if [[ -z "$existing_engine" ]]; then
    echo "==> Creating Agent Runtime ReasoningEngine (${AGENT_DISPLAY_NAME}) in ${REGION} with ${IMAGE}"
    op_resp="$(curl -sS -X POST \
      -H "Authorization: Bearer ${token}" \
      -H "Content-Type: application/json" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${aiplatform_base}/projects/${PROJECT_ID}/locations/${REGION}/reasoningEngines" \
      -d "$engine_payload")"
    op_name="$(python3 -c 'import json, sys; print(json.loads(sys.argv[1]).get("name", ""))' "$op_resp" 2>/dev/null || true)"
  fi

  if [[ -z "$op_name" ]]; then
    echo "Failed to start ReasoningEngine operation: ${op_resp}" >&2
    exit 1
  fi

  echo "==> Waiting for ReasoningEngine operation ${op_name} ..."
  local reasoning_engine_name=""
  while true; do
    token="$(gcp_access_token)"
    local poll_resp status_line
    poll_resp="$(curl -sS \
      -H "Authorization: Bearer ${token}" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${aiplatform_base}/${op_name}")"
    status_line="$(python3 -c '
import json, sys
doc = json.loads(sys.argv[1])
if not doc.get("done"):
    print("PENDING")
elif "error" in doc:
    err = doc["error"]
    print("ERROR\t" + str(err.get("code", "")) + ": " + str(err.get("message", err)))
else:
    resp = doc.get("response") or {}
    print("DONE\t" + str(resp.get("name", "")))
' "$poll_resp")"
    case "$status_line" in
      PENDING)
        sleep 5
        ;;
      ERROR*)
        echo "ReasoningEngine operation failed: ${status_line#ERROR	}" >&2
        exit 1
        ;;
      DONE*)
        reasoning_engine_name="${status_line#DONE	}"
        break
        ;;
    esac
  done

  if [[ -z "$reasoning_engine_name" && -n "$existing_engine" ]]; then
    reasoning_engine_name="$existing_engine"
  fi
  echo "==> Agent Runtime ReasoningEngine ready: ${reasoning_engine_name}"

  local canonical_engine_name="$reasoning_engine_name"
  if [[ "$canonical_engine_name" == projects/${PROJECT_ID}/* && "$project_number" != "$PROJECT_ID" ]]; then
    canonical_engine_name="projects/${project_number}/${canonical_engine_name#projects/${PROJECT_ID}/}"
  fi

  # Optional Stage 2a: Create or update the Gemini Enterprise OAuth authorization resource.
  local auth_resource_name="$existing_agent_auth"
  if [[ -n "$GEMINI_ENTERPRISE_AUTH_ID" ]]; then
    if [[ "$GEMINI_ENTERPRISE_AUTH_ID" == projects/* ]]; then
      auth_resource_name="$GEMINI_ENTERPRISE_AUTH_ID"
    else
      auth_resource_name="projects/${project_number}/locations/${GEMINI_ENTERPRISE_LOCATION}/authorizations/${GEMINI_ENTERPRISE_AUTH_ID}"
    fi
    local oauth_client_id="${OAUTH_CLIENT_ID:-}"
    local oauth_client_secret="${OAUTH_CLIENT_SECRET:-}"
    if [[ -z "$oauth_client_id" ]]; then
      oauth_client_id="$(gcloud secrets versions access latest --secret="${SERVICE}-client-id" --project="$PROJECT_ID" 2>/dev/null || true)"
    fi
    if [[ -z "$oauth_client_secret" ]]; then
      oauth_client_secret="$(gcloud secrets versions access latest --secret="${SERVICE}-client-secret" --project="$PROJECT_ID" 2>/dev/null || true)"
    fi
    if [[ -n "$oauth_client_id" && -n "$oauth_client_secret" ]]; then
      echo "==> Configuring Gemini Enterprise authorization resource ${auth_resource_name}"
      local auth_payload
      auth_payload="$(python3 -c '
import json, sys, urllib.parse

auth_name, client_id, client_secret, scopes = sys.argv[1:5]
redirect_uri = "https://vertexaisearch.cloud.google.com/static/oauth/oauth.html"
params = (
    "client_id=" + urllib.parse.quote(client_id, safe="")
    + "&redirect_uri=" + urllib.parse.quote(redirect_uri, safe="")
    + "&scope=" + urllib.parse.quote(" ".join(scopes.split()), safe="")
    + "&include_granted_scopes=true"
    + "&response_type=code"
    + "&access_type=offline"
    + "&prompt=consent"
)
auth_uri = "https://accounts.google.com/o/oauth2/v2/auth?" + params
print(json.dumps({
    "name": auth_name,
    "serverSideOauth2": {
        "clientId": client_id,
        "clientSecret": client_secret,
        "authorizationUri": auth_uri,
        "tokenUri": "https://oauth2.googleapis.com/token",
    },
}))
' "$auth_resource_name" "$oauth_client_id" "$oauth_client_secret" "$OAUTH_SCOPES")"

      token="$(gcp_access_token)"
      if curl -fsS \
        -H "Authorization: Bearer ${token}" \
        -H "X-Goog-User-Project: ${PROJECT_ID}" \
        "${discovery_base}/${auth_resource_name}" >/dev/null 2>&1; then
        curl -sS -X PATCH \
          -H "Authorization: Bearer ${token}" \
          -H "Content-Type: application/json" \
          -H "X-Goog-User-Project: ${PROJECT_ID}" \
          "${discovery_base}/${auth_resource_name}" \
          -d "$auth_payload" >/dev/null
      else
        curl -sS -X POST \
          -H "Authorization: Bearer ${token}" \
          -H "Content-Type: application/json" \
          -H "X-Goog-User-Project: ${PROJECT_ID}" \
          "${discovery_base}/projects/${project_number}/locations/${GEMINI_ENTERPRISE_LOCATION}/authorizations?authorizationId=${GEMINI_ENTERPRISE_AUTH_ID##*/}" \
          -d "$auth_payload" >/dev/null
      fi
      echo "==> Authorization resource ready: ${auth_resource_name}"
    else
      echo "Note: OAUTH_CLIENT_ID / OAUTH_CLIENT_SECRET not found; assuming authorization ${auth_resource_name} already exists." >&2
    fi
  fi

  # Stage 2b: Register or update the ADK agent on the Gemini Enterprise app.
  if [[ -z "$GEMINI_ENTERPRISE_APP_ID" ]]; then
    echo "==> GEMINI_ENTERPRISE_APP_ID (--app-id) was not provided; skipping Gemini Enterprise app registration."
    echo "    To register this Agent Runtime endpoint with a Gemini Enterprise app, run:"
    echo "    ./deploy.sh --gemini-enterprise --skip-build --engine-id ${reasoning_engine_name##*/} --app-id <YOUR_GEMINI_ENTERPRISE_APP_ID>"
    return 0
  fi

  token="$(gcp_access_token)"
  if [[ -z "$existing_agent_name" && -n "$GEMINI_ENTERPRISE_AGENT_ID" ]]; then
    if [[ "$GEMINI_ENTERPRISE_AGENT_ID" == projects/* ]]; then
      existing_agent_name="$GEMINI_ENTERPRISE_AGENT_ID"
    else
      existing_agent_name="${agents_parent}/${GEMINI_ENTERPRISE_AGENT_ID}"
    fi
  fi

  local agent_payload
  agent_payload="$(python3 -c '
import json, sys
display_name, description, engine_name, auth_res, icon_uri, existing_name = sys.argv[1:7]
body = {
    "displayName": display_name,
    "description": description,
    "adkAgentDefinition": {
        "provisionedReasoningEngine": {
            "reasoningEngine": engine_name,
        },
    },
}
if existing_name:
    body["name"] = existing_name
if icon_uri:
    body["icon"] = {"uri": icon_uri}
if auth_res:
    body["authorizationConfig"] = {"toolAuthorizations": [auth_res]}
print(json.dumps(body))
' "$AGENT_DISPLAY_NAME" "$AGENT_DESCRIPTION" "$canonical_engine_name" "$auth_resource_name" "$AGENT_ICON_URI" "$existing_agent_name")"

  local agent_resp
  if [[ -n "$existing_agent_name" ]]; then
    echo "==> Updating existing Gemini Enterprise agent ${existing_agent_name}"
    agent_resp="$(curl -sS -X PATCH \
      -H "Authorization: Bearer ${token}" \
      -H "Content-Type: application/json" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${discovery_base}/${existing_agent_name}" \
      -d "$agent_payload")"
  else
    echo "==> Registering ADK agent '${AGENT_DISPLAY_NAME}' on Gemini Enterprise app ${GEMINI_ENTERPRISE_APP_ID} (${GEMINI_ENTERPRISE_LOCATION})"
    agent_resp="$(curl -sS -X POST \
      -H "Authorization: Bearer ${token}" \
      -H "Content-Type: application/json" \
      -H "X-Goog-User-Project: ${PROJECT_ID}" \
      "${discovery_base}/${agents_parent}" \
      -d "$agent_payload")"
  fi

  echo "==> Live (Gemini Enterprise):"
  python3 -c '
import json, sys
doc = json.loads(sys.argv[1])
if "error" in doc:
    err = doc["error"]
    sys.stderr.write("Gemini Enterprise agent registration failed: " + json.dumps(err) + "\n")
    sys.exit(1)
print("Agent Resource   : " + str(doc.get("name", "")))
print("Display Name     : " + str(doc.get("displayName", "")))
print("Reasoning Engine : " + str(doc.get("adkAgentDefinition", {}).get("provisionedReasoningEngine", {}).get("reasoningEngine", "")))
print("State            : " + str(doc.get("state", "ENABLED")))
' "$agent_resp"
}

if [[ "$DEPLOY_TARGET" == "cloud-run" || "$DEPLOY_TARGET" == "all" ]]; then
  deploy_cloud_run
fi

if [[ "$DEPLOY_TARGET" == "gemini-enterprise" || "$DEPLOY_TARGET" == "all" ]]; then
  deploy_gemini_enterprise
fi
