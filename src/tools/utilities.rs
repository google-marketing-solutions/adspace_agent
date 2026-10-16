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

//! Utility tools: artifact listing plus adk-rust's built-in `load_artifacts`.

use std::sync::Arc;

use adk_rust::tool::{BasicToolset, LoadArtifactsTool};
use adk_rust::{Result, Tool, ToolContext};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Lists the artifacts available to the user in the current session.
pub struct ListUserFilesTool;

#[async_trait]
impl Tool for ListUserFilesTool {
    fn name(&self) -> &str {
        "list_user_files"
    }

    fn description(&self) -> &str {
        "Tool to list available artifacts (uploaded or generated files) for the user in this session."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, _args: Value) -> Result<Value> {
        let Some(artifacts) = ctx.artifacts() else {
            tracing::warn!("Error listing artifacts: artifact service is not configured.");
            return Ok(Value::String(
                "Error: Could not list artifacts.".to_string(),
            ));
        };
        match artifacts.list().await {
            Ok(files) if files.is_empty() => {
                Ok(Value::String("You have no saved artifacts.".to_string()))
            }
            Ok(files) => {
                let listing = files
                    .iter()
                    .map(|f| format!("- {f}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(Value::String(format!(
                    "Here are your available artifacts:\n{listing}"
                )))
            }
            Err(e) => {
                tracing::warn!("An unexpected error occurred during artifact list: {e}");
                Ok(Value::String(
                    "Error: An unexpected error occurred while listing artifacts.".to_string(),
                ))
            }
        }
    }
}

/// Builds the utilities toolset (an adk-rust [`BasicToolset`] named
/// `utilities`): `list_user_files` plus the built-in `load_artifacts`.
pub fn utilities_toolset() -> BasicToolset {
    BasicToolset::new(
        "utilities",
        vec![
            Arc::new(ListUserFilesTool),
            Arc::new(LoadArtifactsTool::new()),
        ],
    )
}
