// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Generic Google API toolsets driven by Discovery documents.
//!
//! Mirrors the Python ADK `GoogleApiToolset`: a Discovery document is fetched
//! for an API (e.g. `drive` `v3`), every REST method becomes an LLM tool named
//! after its method id (`drive_files_list`), and calls are executed over REST
//! with the end user's OAuth 2.0 credentials.

pub mod discovery;
pub mod naming;
pub mod spec;
pub mod toolset;

pub use discovery::{DiscoveryDocument, DiscoveryFetcher};
pub use spec::{MethodSpec, SpecOptions};
pub use toolset::{GoogleApiTool, GoogleApiToolset, GoogleApiToolsetConfig};
