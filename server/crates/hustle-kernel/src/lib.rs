//! hustle-kernel — cross-cutting, dependency-light runtime support.
//!
//! Owns: process config (env), telemetry init. Nothing here may depend on a
//! domain crate (ADR-001). DB-backed tunables arrive in Phase 1 (`app_config`);
//! until then the server reads defaults from env with economy defaults that
//! mirror the migration seeds (`server/migrations/0001_init.sql`).

pub mod config;
pub mod telemetry;

pub use config::ServerConfig;
