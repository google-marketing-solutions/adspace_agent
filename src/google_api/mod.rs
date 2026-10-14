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
