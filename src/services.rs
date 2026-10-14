//! Session, artifact and memory service construction from environment URIs.
//!
//! Mirrors the `--session_service_uri`, `--artifact_service_uri` and
//! `--memory_service_uri` flags of `adk web` through the environment
//! variables `SESSION_SERVICE_URI`, `ARTIFACT_SERVICE_URI` and
//! `MEMORY_SERVICE_URI`.

use std::sync::Arc;

use adk_rust::artifact::{
    ArtifactService, FileArtifactService, GcsArtifactService, InMemoryArtifactService,
};
use adk_rust::memory::{InMemoryMemoryService, MemoryService};
use adk_rust::session::{InMemorySessionService, SessionService, SqliteSessionService};
use anyhow::Context;

use crate::config::env_opt;

/// Default SQLite session database used when `SESSION_SERVICE_URI` is unset.
pub const DEFAULT_SESSION_DB: &str = "sqlite:///./sessions.db";

/// Normalises Python-ADK style SQLite URIs (`sqlite:///./sessions.db`,
/// `sqlite+aiosqlite:///./sessions.db`) into the `sqlx` form
/// (`sqlite://./sessions.db?mode=rwc`).
pub fn normalize_sqlite_uri(uri: &str) -> String {
    let rest = uri
        .strip_prefix("sqlite+aiosqlite://")
        .or_else(|| uri.strip_prefix("sqlite://"))
        .unwrap_or(uri);
    // `sqlite:///./x.db` → rest = "/./x.db" (relative); `sqlite:////abs.db` → "//abs.db".
    let path = match rest.strip_prefix('/') {
        Some(p) if p.starts_with('/') => p.to_string(),
        Some(p) if p.starts_with("./") || p.starts_with("../") || p.is_empty() => p.to_string(),
        Some(p) => format!("/{p}"),
        None => rest.to_string(),
    };
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (path, None),
    };
    match query {
        Some(q) if q.contains("mode=") => format!("sqlite://{path}?{q}"),
        Some(q) => format!("sqlite://{path}?{q}&mode=rwc"),
        None => format!("sqlite://{path}?mode=rwc"),
    }
}

/// Builds the session service from `SESSION_SERVICE_URI`.
///
/// # Errors
///
/// Returns an error when the SQLite database cannot be opened.
pub async fn build_session_service() -> anyhow::Result<Arc<dyn SessionService>> {
    let uri = env_opt("SESSION_SERVICE_URI").unwrap_or_else(|| DEFAULT_SESSION_DB.to_string());
    if uri.starts_with("memory://") || uri == "memory" {
        tracing::info!("Session service: in-memory");
        return Ok(Arc::new(InMemorySessionService::new()));
    }
    if uri.starts_with("sqlite") {
        let normalized = normalize_sqlite_uri(&uri);
        tracing::info!("Session service: SQLite ({normalized})");
        let service = SqliteSessionService::new(&normalized)
            .await
            .with_context(|| format!("opening session database {normalized}"))?;
        return Ok(Arc::new(service));
    }
    anyhow::bail!(
        "Unsupported SESSION_SERVICE_URI '{uri}'. Use memory:// or sqlite:///path/to/sessions.db"
    )
}

/// Builds the artifact service from `ARTIFACT_SERVICE_URI`.
///
/// Supported: `memory://` (default), `gs://bucket`, `file://./dir` or a plain
/// directory path.
///
/// # Errors
///
/// Returns an error when the backend cannot be initialised.
pub fn build_artifact_service() -> anyhow::Result<Arc<dyn ArtifactService>> {
    let uri = env_opt("ARTIFACT_SERVICE_URI").unwrap_or_else(|| "memory://".to_string());
    if uri.starts_with("memory://") || uri == "memory" {
        tracing::info!("Artifact service: in-memory");
        return Ok(Arc::new(InMemoryArtifactService::new()));
    }
    if let Some(bucket) = uri.strip_prefix("gs://") {
        let bucket = bucket.trim_end_matches('/');
        tracing::info!("Artifact service: GCS bucket {bucket}");
        let service = GcsArtifactService::new_with_adc(bucket)
            .with_context(|| format!("initialising GCS artifact service for {bucket}"))?;
        return Ok(Arc::new(service));
    }
    let dir = uri.strip_prefix("file://").unwrap_or(&uri);
    tracing::info!("Artifact service: directory {dir}");
    let service = FileArtifactService::new(dir)
        .with_context(|| format!("initialising file artifact service at {dir}"))?;
    Ok(Arc::new(service))
}

/// Builds the memory service from `MEMORY_SERVICE_URI` (only `memory://` is
/// supported today).
///
/// # Errors
///
/// Returns an error for unsupported URIs.
pub fn build_memory_service() -> anyhow::Result<Arc<dyn MemoryService>> {
    let uri = env_opt("MEMORY_SERVICE_URI").unwrap_or_else(|| "memory://".to_string());
    if uri.starts_with("memory://") || uri == "memory" {
        tracing::info!("Memory service: in-memory");
        return Ok(Arc::new(InMemoryMemoryService::new()));
    }
    anyhow::bail!("Unsupported MEMORY_SERVICE_URI '{uri}'. Only memory:// is supported")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sqlite_uris() {
        assert_eq!(
            normalize_sqlite_uri("sqlite:///./sessions.db"),
            "sqlite://./sessions.db?mode=rwc"
        );
        assert_eq!(
            normalize_sqlite_uri("sqlite+aiosqlite:///./sessions.db"),
            "sqlite://./sessions.db?mode=rwc"
        );
        assert_eq!(
            normalize_sqlite_uri("sqlite:////tmp/s.db"),
            "sqlite:///tmp/s.db?mode=rwc"
        );
        assert_eq!(
            normalize_sqlite_uri("sqlite://./s.db?mode=rwc"),
            "sqlite://./s.db?mode=rwc"
        );
        assert_eq!(
            normalize_sqlite_uri("sqlite:///data/s.db"),
            "sqlite:///data/s.db?mode=rwc"
        );
    }
}
