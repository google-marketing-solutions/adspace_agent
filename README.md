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
# Required for: `adspace_agent/tools/google_ads.py`
# Reference:
# https://developers.google.com/google-ads/api/docs/client-libs/python/configuration#env-config-fields
# https://github.com/googleads/google-ads-python/blob/HEAD/google-ads.yaml
# https://developers.google.com/google-ads/api/docs/api-policy/developer-token
export GOOGLE_ADS_DEVELOPER_TOKEN=
export GOOGLE_ADS_LOGIN_CUSTOMER_ID=
```

When deploying, you can set those environment variables in the Google Cloud user
interface.

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

## Deployment

To deploy the application, you can use the Google Cloud user interface to set
the environment variables. Refer to the previous section for instructions on how
to set up the environment variables.

```shell
gcloud run deploy adspace-agent \
  --source . \
  --memory 4Gi \
  --port 8000
```

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

## Contributing

Want to contribute? [Learn more](CONTRIBUTING.md)
