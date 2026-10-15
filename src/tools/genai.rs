//! Tools that call Google generative media APIs (Gemini, Imagen, Veo).

use std::sync::Arc;

use adk_rust::tool::BasicToolset;
use adk_rust::{AdkError, Part, Result, Tool, ToolContext};
use async_trait::async_trait;
use serde_json::{Value, json};

use crate::config::{ModelController, SimpleModelController};
use crate::genai::{GenAiClient, GeneratedMedia};

/// Builds a Python-compatible error payload.
fn error_response(details: impl std::fmt::Display) -> Value {
    json!({ "status": "ERROR", "error_details": details.to_string() })
}

/// Saves generated media as a session artifact.
async fn save_media_artifact(
    ctx: &Arc<dyn ToolContext>,
    filename: &str,
    media: GeneratedMedia,
) -> anyhow::Result<()> {
    let artifacts = ctx
        .artifacts()
        .ok_or_else(|| anyhow::anyhow!("Artifact service is not configured."))?;
    // Built directly (not via `Part::inline_data`) so large videos do not trip
    // the inline-size assertion meant for model requests.
    let part = Part::InlineData {
        mime_type: media.mime_type,
        data: media.data,
        uri: None,
        annotations: None,
    };
    let version = artifacts.save(filename, &part).await?;
    let mut actions = ctx.actions();
    actions.artifact_delta.insert(filename.to_string(), version);
    ctx.set_actions(actions);
    Ok(())
}

/// Asks a question about a YouTube video.
pub struct GetInfoAboutYoutubeVideoTool {
    client: GenAiClient,
    models: Arc<dyn ModelController>,
}

#[async_trait]
impl Tool for GetInfoAboutYoutubeVideoTool {
    fn name(&self) -> &str {
        "get_info_about_youtube_video"
    }

    fn description(&self) -> &str {
        "Gets info about a YouTube video based on a prompt. This function gives the ability to prompt a \
         YouTube video and ask questions about the video's visual, audio, and more. It also provides \
         answers to the prompt. This function is optimized for parallel execution - call multiple times \
         for different videos if needed."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "youtube_video_id": {
                    "type": "string",
                    "description": "The external YouTube video ID, found at the end of a YouTube URL in the `v` parameter. For example, in the URL `https://www.youtube.com/watch?v=abc123`, the `youtube_video_id` would be `abc123`."
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt used to ask the YouTube video questions about itself."
                }
            },
            "required": ["youtube_video_id", "prompt"]
        }))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let video_id = args
            .get("youtube_video_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let prompt = args
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if video_id.is_empty() {
            return Err(AdkError::tool("youtube_video_id is required"));
        }
        let model = self.models.active_models().model;
        Ok(
            match self
                .client
                .ask_youtube_video(&model, &video_id, &prompt)
                .await
            {
                Ok(text) => json!({ "status": "SUCCESS", "response": text }),
                Err(e) => error_response(format!("{e:#}")),
            },
        )
    }
}

/// Generates a video with Veo and stores it as an artifact.
pub struct GenerateVideoTool {
    client: GenAiClient,
    models: Arc<dyn ModelController>,
}

#[async_trait]
impl Tool for GenerateVideoTool {
    fn name(&self) -> &str {
        "generate_video"
    }

    fn description(&self) -> &str {
        "Generates a video from a prompt using Google GenAI's video model and saves it as an artifact. \
         This function is optimized for parallel execution."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "The prompt to use for creating the video." }
            },
            "required": ["prompt"]
        }))
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let prompt = args
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let video_model = self.models.active_models().video_model;
        let media = match self.client.generate_video(&video_model, &prompt).await {
            Ok(media) => media,
            Err(e) => return Ok(error_response(format!("{e:#}"))),
        };
        let filename = format!("generated_video_{}.mp4", uuid::Uuid::new_v4().simple());
        Ok(match save_media_artifact(&ctx, &filename, media).await {
            Ok(()) => {
                json!({ "status": "SUCCESS", "message": "Generated video.", "filename": filename })
            }
            Err(e) => error_response(format!("{e:#}")),
        })
    }
}

/// Generates an image and stores it as an artifact.
pub struct GenerateImageTool {
    client: GenAiClient,
    models: Arc<dyn ModelController>,
}

#[async_trait]
impl Tool for GenerateImageTool {
    fn name(&self) -> &str {
        "generate_image"
    }

    fn description(&self) -> &str {
        "Generates an image from a prompt using Google GenAI's image model and saves it as an artifact. \
         This function is optimized for parallel execution."
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "The prompt to use for creating the image." }
            },
            "required": ["prompt"]
        }))
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: Arc<dyn ToolContext>, args: Value) -> Result<Value> {
        let prompt = args
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let image_model = self.models.active_models().image_model;
        let media = match self.client.generate_image(&image_model, &prompt).await {
            Ok(media) => media,
            Err(e) => return Ok(error_response(format!("{e:#}"))),
        };
        let extension = match media.mime_type.as_str() {
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => "png",
        };
        let filename = format!(
            "generated_image_{}.{extension}",
            uuid::Uuid::new_v4().simple()
        );
        Ok(match save_media_artifact(&ctx, &filename, media).await {
            Ok(()) => {
                json!({ "status": "SUCCESS", "message": "Generated image.", "filename": filename })
            }
            Err(e) => error_response(format!("{e:#}")),
        })
    }
}

/// Builds the Google GenAI toolset (an adk-rust [`BasicToolset`] named
/// `google_genai`).
///
/// `model` answers YouTube questions, `video_model` is the Veo model and
/// `image_model` the image model.
pub fn google_genai_toolset(
    client: GenAiClient,
    model: impl Into<String>,
    video_model: impl Into<String>,
    image_model: impl Into<String>,
) -> BasicToolset {
    let controller: Arc<dyn ModelController> =
        Arc::new(SimpleModelController::new(model, video_model, image_model));
    google_genai_toolset_with_controller(client, controller)
}

/// Builds the Google GenAI toolset backed by a dynamic [`ModelController`] so
/// model changes made in the Studio settings take effect immediately.
pub fn google_genai_toolset_with_controller(
    client: GenAiClient,
    models: Arc<dyn ModelController>,
) -> BasicToolset {
    BasicToolset::new(
        "google_genai",
        vec![
            Arc::new(GetInfoAboutYoutubeVideoTool {
                client: client.clone(),
                models: models.clone(),
            }),
            Arc::new(GenerateVideoTool {
                client: client.clone(),
                models: models.clone(),
            }),
            Arc::new(GenerateImageTool { client, models }),
        ],
    )
}
