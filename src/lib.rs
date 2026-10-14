//! AdSpace Agent — a Rust port of the AdSpace Agent built on adk-rust.
//!
//! AdSpace Agent is designed to provide a standardized way to integrate an LLM
//! with Google Ads, YouTube, Google Cloud, and Google Search to form a more
//! comprehensive campaign and marketing plan for agencies.

pub mod agent;
pub mod auth;
pub mod config;
pub mod gcs;
pub mod genai;
pub mod google_api;
pub mod server;
pub mod services;
pub mod tools;
