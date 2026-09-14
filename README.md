**This is not an officially supported Google product.**

<img src="assets/logo.png" width="400" height="400">

# AdSpace Agent

AdSpace Agent is designed to provide a standardized way to integrate an LLM with
Google Ads, YouTube, Google Cloud, and Google Search to form a more
comprehensive campaign and marketing plan for agencies.

[![Continuous Integration](https://github.com/google-marketing-solutions/adspace_agent/actions/workflows/ci.yml/badge.svg)](https://github.com/google-marketing-solutions/adspace_agent/actions/workflows/ci.yml)
[![Code Style: Google](https://img.shields.io/badge/code%20style-google-4285F4.svg)](https://google.github.io/styleguide/pyguide.html)
[![Conventional Commits](https://img.shields.io/badge/conventional%20commits-1.0.0-fe5196.svg?logo=conventionalcommits)](https://conventionalcommits.org)
[![prek](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/j178/prek/master/docs/assets/badge-v0.json)](https://github.com/j178/prek)

## Getting Started

### Environment Variables

Create a `.env` file in the root of the project. Here are the environment
variables required for the project:

```shell
# ADK
export GOOGLE_CLOUD_LOCATION=global
export GOOGLE_CLOUD_PROJECT=
export GOOGLE_GENAI_USE_VERTEXAI=TRUE

# Google API Toolsets
export CLIENT_ID=
export CLIENT_SECRET=

# Google Ads
# Reference:
# https://developers.google.com/google-ads/api/docs/client-libs/python/configuration#env-config-fields
# https://github.com/googleads/google-ads-python/blob/HEAD/google-ads.yaml
# https://developers.google.com/google-ads/api/docs/api-policy/developer-token
export GOOGLE_ADS_DEVELOPER_TOKEN=
export GOOGLE_ADS_LOGIN_CUSTOMER_ID=

# Optionals:
# export MODEL=gemini-3.6-flash
# export VEO_MODEL=veo-3.1-fast-generate-001
# export IMAGEN_MODEL=imagen-4.0-fast-generate-001
# export ENABLED_TOOLSETS=bid_manager,bigquery,campaign_manager_360,display_video_360,drive,merchant_center_inventories,merchant_center_products,merchant_center_reports,search_ads_360,storage,youtube,google_ads,google_genai
# export GOOGLE_ADS_TOOL_FILTER=googleads_customers_google_ads_search,googleads_google_ads_fields_search
# export SKILLS_BUCKET_NAME="adspace-agent"
# export LOCAL_SKILLS_DIR="./skills"
# export OPTIONAL_ADK_WEB_ARGS="--logo-text \"AdSpace Agent\" --logo-image-url \"https://raw.githubusercontent.com/google-marketing-solutions/adspace_agent/main/assets/logo.png\" --eval_storage_uri \"gs://adspace-agent\" --session_service_uri \"sqlite+aiosqlite:///./sessions.db\" --artifact_service_uri \"gs://adspace-agent\" --memory_service_uri \"memory://\""
```

See: `.env.sample` for more details. When deploying, you can set those
environment variables in the Google Cloud user interface.

### Development

Install the dependencies with `uv` including all development dependencies:

```shell
uv sync --all-extras
```

Run the ADK webserver locally:

```shell
uv run adk web
```

Follow the URL to open the web interface.

## Exploring Available Tools

You can list all available tools for supported APIs using the `list-tools`
utility command. This is useful for discovering what operations the agent can
perform.

### Google Ads Tools

```shell
uv run list-tools google_ads
```

### YouTube Tools

```shell
uv run list-tools youtube
```

### Other Google API Tools

```shell
uv run list-tools google --api <api_name> --version <version>
```

Example for Google Drive:

```shell
uv run list-tools google --api drive --version v3
```

## Configuring Your GCS Bucket for Agent Skills

You can configure the agent to dynamically load custom skills from a Google Cloud Storage (GCS) bucket by setting the `SKILLS_BUCKET_NAME` environment variable (for example, `SKILLS_BUCKET_NAME={YOUR_SKILLS_BUCKET_NAME}`).

### Required Bucket Folder Structure

The agent expects a top-level `skills/` directory in your GCS bucket. Inside `skills/`, each skill must be placed in its own subfolder:

- The **name of the folder** must be the name of the skill in standard skill format (kebab-case, e.g., `cm360-trafficking`).
- The folder name **must exactly match** the `name` field declared in the YAML frontmatter of the `SKILL.md` file inside that folder.

For example, for the `cm360-trafficking` skill, your bucket structure should look like this:

```text
gs://{YOUR_SKILLS_BUCKET_NAME}/
└── skills/
    └── cm360-trafficking/
        └── SKILL.md
```

And the corresponding `SKILL.md` file inside `skills/cm360-trafficking/SKILL.md` must have matching frontmatter:

```yaml
---
name: cm360-trafficking
description: Use this skill ONLY when the user requests something related to trafficking, pushing, or editing campaigns in Campaign Manager 360 (CM360)...
---
```

## Deployment

To deploy the application, you can set your environment variables either through the Google Cloud Console or directly via the `gcloud run deploy` CLI command using `--set-env-vars`. Refer to the [Environment Variables](#environment-variables) section for details on each variable.

You will also need the following APIs enabled:

```shell
gcloud services enable \
  aiplatform.googleapis.com \
  bigquery.googleapis.com \
  cloudbuild.googleapis.com \
  dfareporting.googleapis.com \
  displayvideo.googleapis.com \
  doubleclickbidmanager.googleapis.com \
  drive.googleapis.com \
  googleads.googleapis.com \
  run.googleapis.com \
  merchantapi.googleapis.com \
  searchads360.googleapis.com \
  storage.googleapis.com \
  youtube.googleapis.com
```

### Cloud Run

To deploy to Cloud Run and pass the required environment variables:

```shell
gcloud run deploy adspace-agent \
  --source . \
  --region us-central1 \
  --memory 4Gi \
  --cpu 1 \
  --port 8000 \
  --set-env-vars "\
GOOGLE_CLOUD_LOCATION=global,\
GOOGLE_CLOUD_PROJECT={YOUR_GOOGLE_CLOUD_PROJECT},\
GOOGLE_GENAI_USE_VERTEXAI=TRUE,\
GOOGLE_ADS_DEVELOPER_TOKEN={YOUR_GOOGLE_ADS_DEVELOPER_TOKEN},\
GOOGLE_ADS_LOGIN_CUSTOMER_ID={YOUR_GOOGLE_ADS_LOGIN_CUSTOMER_ID},\
CLIENT_ID={YOUR_CLIENT_ID},\
CLIENT_SECRET={YOUR_CLIENT_SECRET},\
SKILLS_BUCKET_NAME={YOUR_SKILLS_BUCKET_NAME}"
```

## Contributing

Want to contribute? [Learn more](CONTRIBUTING.md)
