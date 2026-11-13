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

//! Process logging.
//!
//! Two formats, chosen once at start-up by `LOG_FORMAT`:
//!
//! | `LOG_FORMAT` | Output | Tracing |
//! |---|---|---|
//! | `text` | adk-rust's console lines | adk-rust's in-process span exporter, which backs the `/ui/` Trace tab |
//! | `json` | One JSON object per line, in the shape Cloud Logging parses (`severity`, `message`, `timestamp`, source location, the event's fields) | None: the span exporter keeps every span in memory and nothing in a deployment reads it |
//!
//! Unset, the format is `json` on Cloud Run (which sets `K_SERVICE`) and
//! `text` everywhere else. With `text`, Cloud Logging stores each line as a
//! string with no severity, so a severity filter matches nothing.
//!
//! The level filter is `RUST_LOG` in both formats.

use std::fmt;
use std::sync::OnceLock;

use serde_json::{Map, Value, json};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::{Layer as _, SubscriberExt as _};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt as _;

/// `RUST_LOG` filter when the operator did not set one.
pub const DEFAULT_LOG_FILTER: &str = "info,sqlx=warn,hyper=warn,h2=warn,reqwest=warn";

/// How log lines are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Console lines for a person at a terminal.
    Text,
    /// One Cloud Logging JSON object per line.
    Json,
}

impl LogFormat {
    /// Picks the format from `LOG_FORMAT` (`json` or `text`), falling back
    /// to `json` on Cloud Run and `text` elsewhere.
    ///
    /// The second value is a complaint about an unusable `LOG_FORMAT`, for
    /// the caller to report once logging is up.
    pub fn resolve(setting: Option<&str>, on_cloud_run: bool) -> (Self, Option<String>) {
        let fallback = if on_cloud_run { Self::Json } else { Self::Text };
        match setting.map(str::trim).filter(|s| !s.is_empty()) {
            None => (fallback, None),
            Some(s) if s.eq_ignore_ascii_case("json") => (Self::Json, None),
            Some(s) if s.eq_ignore_ascii_case("text") => (Self::Text, None),
            Some(other) => (
                fallback,
                Some(format!(
                    "Ignoring LOG_FORMAT={other:?}; valid values are json and text"
                )),
            ),
        }
    }
}

static FORMAT: OnceLock<LogFormat> = OnceLock::new();

/// The format [`init`] installed; `Text` before it has run.
pub fn format() -> LogFormat {
    FORMAT.get().copied().unwrap_or(LogFormat::Text)
}

/// Installs the process-wide logger. Call once, from `main`, before any
/// other thread exists: it may set `RUST_LOG`.
pub fn init(service_name: &str) {
    if std::env::var_os("RUST_LOG").is_none() {
        // SAFETY: called from `main` before any other thread exists.
        unsafe { std::env::set_var("RUST_LOG", DEFAULT_LOG_FILTER) };
    }
    let (format, complaint) = LogFormat::resolve(
        std::env::var("LOG_FORMAT").ok().as_deref(),
        std::env::var_os("K_SERVICE").is_some(),
    );
    let _ = FORMAT.set(format);
    match format {
        LogFormat::Text => {
            if let Err(e) = adk_rust::telemetry::init_with_adk_exporter(service_name) {
                eprintln!("warning: telemetry initialization failed: {e}");
            }
        }
        LogFormat::Json => {
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));
            let installed = tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .event_format(CloudLoggingFormat)
                        .with_filter(filter),
                )
                .try_init();
            if let Err(e) = installed {
                eprintln!("warning: logging initialization failed: {e}");
            }
        }
    }
    if let Some(complaint) = complaint {
        tracing::warn!("{complaint}");
    }
}

/// The Cloud Logging severity for a tracing level. Cloud Logging has no
/// `TRACE` and spells warnings `WARNING`.
fn severity(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARNING",
        Level::INFO => "INFO",
        _ => "DEBUG",
    }
}

/// Collects an event's fields as JSON, with the message set apart.
#[derive(Default)]
struct JsonFields {
    message: Option<String>,
    fields: Map<String, Value>,
}

impl JsonFields {
    fn put(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.message = Some(match value {
                Value::String(text) => text,
                other => other.to_string(),
            });
        } else {
            self.fields.insert(field.name().to_string(), value);
        }
    }
}

impl Visit for JsonFields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, json!(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, json!(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, json!(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, json!(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, json!(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, json!(value));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.put(field, json!(value.to_string()));
    }
}

/// Writes an event as one line of the JSON that Cloud Logging turns into a
/// structured entry: `severity`, `message` and `timestamp` are lifted into
/// the entry, the rest lands in its `jsonPayload`.
pub struct CloudLoggingFormat;

/// The JSON object [`CloudLoggingFormat`] writes for `event`, stamped `timestamp`.
fn entry(event: &Event<'_>, timestamp: String) -> Value {
    let metadata = event.metadata();
    let mut collected = JsonFields::default();
    event.record(&mut collected);

    let mut entry = Map::new();
    entry.insert("severity".into(), json!(severity(metadata.level())));
    entry.insert(
        "message".into(),
        json!(collected.message.unwrap_or_default()),
    );
    entry.insert("timestamp".into(), json!(timestamp));
    entry.insert("target".into(), json!(metadata.target()));
    if let (Some(file), Some(line)) = (metadata.file(), metadata.line()) {
        entry.insert(
            "logging.googleapis.com/sourceLocation".into(),
            json!({ "file": file, "line": line.to_string() }),
        );
    }
    // An event field never replaces one of the keys above.
    for (name, value) in collected.fields {
        entry.entry(name).or_insert(value);
    }
    Value::Object(entry)
}

impl<S, N> FormatEvent<S, N> for CloudLoggingFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        writeln!(writer, "{}", entry(event, timestamp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    #[test]
    fn format_follows_the_setting_then_the_platform() {
        use LogFormat::{Json, Text};
        assert_eq!(LogFormat::resolve(None, false), (Text, None));
        assert_eq!(LogFormat::resolve(None, true), (Json, None));
        assert_eq!(LogFormat::resolve(Some(""), true), (Json, None));
        assert_eq!(LogFormat::resolve(Some("json"), false), (Json, None));
        assert_eq!(LogFormat::resolve(Some(" TEXT "), true), (Text, None));

        let (format, complaint) = LogFormat::resolve(Some("yaml"), true);
        assert_eq!(format, Json);
        assert!(complaint.unwrap().contains("LOG_FORMAT=\"yaml\""));
    }

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// What the JSON logger writes for the events `emit` raises.
    fn logged(emit: impl FnOnce()) -> Vec<Value> {
        let capture = Capture::default();
        let sink = capture.clone();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || sink.clone())
                .event_format(CloudLoggingFormat),
        );
        tracing::subscriber::with_default(subscriber, emit);
        let bytes = capture.0.lock().unwrap().clone();
        String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is one JSON object"))
            .collect()
    }

    #[test]
    fn events_become_cloud_logging_entries() {
        let entries = logged(|| {
            tracing::warn!(
                tool = "bigquery_execute_sql",
                attempts = 2,
                "tool call failed"
            );
            tracing::info!("listening on {}", 8000);
            tracing::error!(severity = "spoofed", "a field cannot replace the severity");
            tracing::debug!("fine detail");
        });
        assert_eq!(entries.len(), 4);

        let warning = &entries[0];
        assert_eq!(warning["severity"], "WARNING");
        assert_eq!(warning["message"], "tool call failed");
        assert_eq!(warning["tool"], "bigquery_execute_sql");
        assert_eq!(warning["attempts"], 2);
        assert_eq!(warning["target"], module_path!());
        let location = &warning["logging.googleapis.com/sourceLocation"];
        assert!(location["file"].as_str().unwrap().ends_with("logging.rs"));
        assert!(location["line"].as_str().unwrap().parse::<u32>().is_ok());
        let timestamp = warning["timestamp"].as_str().unwrap();
        assert!(chrono::DateTime::parse_from_rfc3339(timestamp).is_ok());

        // Formatted arguments arrive as plain text, without quoting.
        assert_eq!(entries[1]["severity"], "INFO");
        assert_eq!(entries[1]["message"], "listening on 8000");

        assert_eq!(entries[2]["severity"], "ERROR");
        assert_eq!(entries[3]["severity"], "DEBUG");
    }

    #[test]
    fn a_message_with_newlines_and_quotes_stays_on_one_line() {
        let entries = logged(|| {
            tracing::error!("request failed:\n  \"quota\" exceeded");
        });
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["message"],
            "request failed:\n  \"quota\" exceeded"
        );
    }
}
