//! Session, artifact and memory service construction from environment URIs.
//!
//! Mirrors the `--session_service_uri`, `--artifact_service_uri` and
//! `--memory_service_uri` flags of `adk web` through the environment
//! variables `SESSION_SERVICE_URI`, `ARTIFACT_SERVICE_URI` and
//! `MEMORY_SERVICE_URI`.

use std::sync::Arc;
use std::time::Duration;

use adk_rust::AdkError;
use adk_rust::artifact::{
    ArtifactService, FileArtifactService, GcsArtifactService, InMemoryArtifactService,
};
use adk_rust::identity::AdkIdentity;
use adk_rust::memory::{InMemoryMemoryService, MemoryService};
use adk_rust::session::{
    AppendEventRequest, CreateRequest, DeleteRequest, Event, GetRequest, InMemorySessionService,
    ListRequest, Session, SessionService, SqliteSessionService,
};
use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::RwLock;

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
        // Creates/updates the `sessions`/`events` tables (idempotent).
        service
            .migrate()
            .await
            .with_context(|| format!("migrating session database {normalized}"))?;
        return Ok(Arc::new(SerializedSqliteSessionService::new(service)));
    }
    anyhow::bail!(
        "Unsupported SESSION_SERVICE_URI '{uri}'. Use memory:// or sqlite:///path/to/sessions.db"
    )
}

/// Wraps [`SqliteSessionService`] with an async reader-writer lock and retry
/// backoff so concurrent requests never hit SQLite `BEGIN DEFERRED`
/// read-before-write lock-upgrade deadlocks (`(code: 5) database is locked`).
pub struct SerializedSqliteSessionService {
    inner: SqliteSessionService,
    lock: RwLock<()>,
}

impl SerializedSqliteSessionService {
    /// Wraps an initialized [`SqliteSessionService`].
    pub fn new(inner: SqliteSessionService) -> Self {
        Self {
            inner,
            lock: RwLock::new(()),
        }
    }
}

fn is_sqlite_locked(err: &AdkError) -> bool {
    let msg = err.to_string();
    msg.contains("database is locked") || msg.contains("SQLITE_BUSY")
}

#[async_trait]
impl SessionService for SerializedSqliteSessionService {
    async fn create(&self, req: CreateRequest) -> adk_rust::Result<Box<dyn Session>> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.create(req.clone()).await {
                Ok(session) => return Ok(session),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn get(&self, req: GetRequest) -> adk_rust::Result<Box<dyn Session>> {
        let _guard = self.lock.read().await;
        for attempt in 1..=5 {
            match self.inner.get(req.clone()).await {
                Ok(session) => return Ok(session),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn list(&self, req: ListRequest) -> adk_rust::Result<Vec<Box<dyn Session>>> {
        let _guard = self.lock.read().await;
        for attempt in 1..=5 {
            match self.inner.list(req.clone()).await {
                Ok(sessions) => return Ok(sessions),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn delete(&self, req: DeleteRequest) -> adk_rust::Result<()> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.delete(req.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn append_event(&self, session_id: &str, event: Event) -> adk_rust::Result<()> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.append_event(session_id, event.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn get_for_identity(&self, identity: &AdkIdentity) -> adk_rust::Result<Box<dyn Session>> {
        let _guard = self.lock.read().await;
        for attempt in 1..=5 {
            match self.inner.get_for_identity(identity).await {
                Ok(session) => return Ok(session),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn delete_for_identity(&self, identity: &AdkIdentity) -> adk_rust::Result<()> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.delete_for_identity(identity).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn append_event_for_identity(&self, req: AppendEventRequest) -> adk_rust::Result<()> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.append_event_for_identity(req.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn delete_all_sessions(&self, app_name: &str, user_id: &str) -> adk_rust::Result<()> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.delete_all_sessions(app_name, user_id).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn rewind(
        &self,
        session_id: &str,
        target_event_id: &str,
    ) -> adk_rust::Result<Box<dyn Session>> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.rewind(session_id, target_event_id).await {
                Ok(session) => return Ok(session),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn rewind_steps(
        &self,
        session_id: &str,
        steps: usize,
    ) -> adk_rust::Result<Box<dyn Session>> {
        let _guard = self.lock.write().await;
        for attempt in 1..=5 {
            match self.inner.rewind_steps(session_id, steps).await {
                Ok(session) => return Ok(session),
                Err(e) if attempt < 5 && is_sqlite_locked(&e) => {
                    tokio::time::sleep(Duration::from_millis(20 * attempt)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn health_check(&self) -> adk_rust::Result<()> {
        let _guard = self.lock.read().await;
        self.inner.health_check().await
    }
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

    #[tokio::test]
    async fn fresh_sqlite_database_is_migrated_and_usable() {
        use adk_rust::session::{CreateRequest, ListRequest};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.db");
        let url = normalize_sqlite_uri(&format!("sqlite:///{}", db.display()));
        let service = SqliteSessionService::new(&url).await.unwrap();
        service.migrate().await.unwrap();
        service
            .create(CreateRequest {
                app_name: "app".into(),
                user_id: "u".into(),
                session_id: None,
                state: Default::default(),
            })
            .await
            .unwrap();
        let listed = service
            .list(ListRequest {
                app_name: "app".into(),
                user_id: "u".into(),
                limit: None,
                offset: None,
            })
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn serialized_sqlite_handles_concurrent_session_creates() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sessions.db");
        let url = normalize_sqlite_uri(&format!("sqlite:///{}", db.display()));
        let inner = SqliteSessionService::new(&url).await.unwrap();
        inner.migrate().await.unwrap();
        let service = Arc::new(SerializedSqliteSessionService::new(inner));

        let mut handles = Vec::new();
        for _ in 0..16 {
            let svc = service.clone();
            handles.push(tokio::spawn(async move {
                svc.create(CreateRequest {
                    app_name: "adspace_agent".into(),
                    user_id: "coconate@google.com".into(),
                    session_id: None,
                    state: Default::default(),
                })
                .await
            }));
        }
        for h in handles {
            h.await
                .unwrap()
                .expect("concurrent session create must succeed");
        }

        let listed = service
            .list(ListRequest {
                app_name: "adspace_agent".into(),
                user_id: "coconate@google.com".into(),
                limit: None,
                offset: None,
            })
            .await
            .unwrap();
        assert_eq!(listed.len(), 16);
    }
}
