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

# ---- build stage -----------------------------------------------------------
FROM rust:1.95-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# ---- runtime stage ---------------------------------------------------------
FROM python:3.12-slim-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/*
# Scientific Python stack for the `python_repl_ast` data-analysis tool
# (same libraries the Python agent shipped).
COPY requirements-python.txt /tmp/requirements-python.txt
RUN pip install --no-cache-dir -r /tmp/requirements-python.txt
WORKDIR /app
COPY --from=builder /app/target/release/adspace-agent /usr/local/bin/adspace-agent
COPY assets ./assets
ENV HOST=0.0.0.0 PORT=8000 RUST_LOG=info
EXPOSE 8000
ENTRYPOINT ["adspace-agent"]
CMD ["serve"]
