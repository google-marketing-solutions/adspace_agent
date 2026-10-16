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
FROM rust:1.99-trixie AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    libdbus-1-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# ---- runtime stage ---------------------------------------------------------
# Python code runs server-side in Gemini (PYTHON_EXECUTOR=gemini, the default),
# so the runtime image needs nothing beyond the binary, D-Bus (for adk-cli's
# keyring dependency), and CA certificates.
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libdbus-1-3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/adspace-agent /usr/local/bin/adspace-agent
COPY assets ./assets
ENV HOST=0.0.0.0 PORT=8000 RUST_LOG=info
EXPOSE 8000
ENTRYPOINT ["adspace-agent"]
CMD ["serve"]
