**This is not an officially supported Google product.**

<img src="assets/logo.png" width="400" height="400">

# AdSpace Agent

AdSpace Agent is designed to provide a standardized way to integrate an LLM with
Google Ads, YouTube, Google Cloud, and Google Search to form a more
comprehensive campaign and marketing plan for agencies.

[![Continuous Integration](https://github.com/google-marketing-solutions/google_ads_mcp_server/actions/workflows/ci.yml/badge.svg)](https://github.com/google-marketing-solutions/adspace_agent/actions/workflows/ci.yml)
[![Code Style: Google](https://img.shields.io/badge/code%20style-google-4285F4.svg)](https://google.github.io/styleguide/pyguide.html)
[![Conventional Commits](https://img.shields.io/badge/conventional%20commits-1.0.0-fe5196.svg?logo=conventionalcommits)](https://conventionalcommits.org)
[![pre-commit](https://img.shields.io/badge/pre--commit-enabled-brightgreen?logo=pre-commit)](https://github.com/pre-commit/pre-commit)

## Setup

### Google Ads

Use
[this documentation](https://developers.google.com/google-ads/api/docs/oauth/service-accounts)
to set up a service account for the Google Ads API client library. This will
require you to also set up a Google Cloud project and enable the Google Ads API.
Create a `google-ads.yaml` file as defined
[here](https://github.com/googleads/google-ads-python/blob/HEAD/google-ads.yaml).
Here's an example:

```yaml
# google-ads.yaml
developer_token: INSERT_DEVELOPER_TOKEN_HERE
login_customer_id: INSERT_LOGIN_CUSTOMER_ID_HERE
json_key_file_path: JSON_KEY_FILE_PATH_HERE
use_proto_plus: true
```

### Server

Run the ADK webserver locally:

```shell
uv run adk web
```

### GoogleApiToolkit

The APIs and Versions that can be used with this class can be found running the
below:

```
from googleapiclient import discovery

service = discovery.build('discovery', 'v1')

request = service.apis().list()
response = request.execute()

for api in response.get('items', []):
  print(api.get('name'), api.get('version'))
```

## Contributing

Want to contribute? [Learn more](CONTRIBUTING.md)
