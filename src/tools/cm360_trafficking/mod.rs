//! Campaign Manager 360 (CM360) trafficking tools and utilities.
//!
//! This module parses a CM360 trafficking sheet (CSV), validates it, groups
//! the rows into placements, creatives, event tags and ads, reconciles them
//! against the live CM360 state to build a list of insert/patch operations,
//! and finally executes those operations against the `dfareporting` v5 API.
//!
//! Module layout (mirrors the original Python package):
//!
//! - [`sheet`] — CSV loading, header detection, cleaning and metadata
//!   extraction (`utilities.py`).
//! - [`validations`] — per-row validation rules (`validations.py`).
//! - [`grouping`] — grouping rows into CM360 entity payloads (`actions.py`).
//! - [`client`] — the `dfareporting` v5 REST client (`actions.py`).
//! - [`diff`] — diffing sheet entities against CM360 and building operations
//!   (`helpers.py`).
//! - [`execute`] — executing operations and summarising results
//!   (`helpers.py`).
//! - [`tools`] — the ADK tools and toolset (`trafficking.py`).

pub mod client;
pub mod diff;
pub mod execute;
pub mod grouping;
pub mod sheet;
pub mod tools;
pub mod validations;

pub use tools::{
    CM360_TRAFFICKING_INSTRUCTIONS, Cm360Services, Cm360TraffickingToolset, ParseSheetTool,
    TrafficCampaignsTool,
};
