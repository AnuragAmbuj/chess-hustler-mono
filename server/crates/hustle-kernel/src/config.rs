//! Process configuration from environment variables (12-factor).
//!
//! Every value has a default that matches the seeded `app_config` rows so a
//! bare `cargo run` reproduces the documented economy (docs/04-economy/*).
//! Runtime *tunables* move to DB-backed `app_config` in Phase 1 — these env
//! values are bootstrap/static config only, never hot rules.

use std::env;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Deployment environment name: local | staging | prod.
    pub env: String,
    pub host: String,
    pub port: u16,
    pub version: &'static str,

    // --- Bootstrap/static config ---
    pub database_url: String,
    pub redis_url: String,
    /// HS256 signing secret for access JWTs. MUST be overridden via
    /// `CH_JWT_SECRET` outside local; the default exists only so a bare
    /// `cargo run` works in development.
    pub jwt_secret: String,

    // --- Public economy defaults (mirrored by /v1/meta/config; enforced server-side later) ---
    pub welcome_gold: i64,
    pub ads_gold_per_ad: i64,
    pub ads_max_per_day: u32,
    pub ads_cooldown_s: u64,

    // --- Production runtime adapters & guards ---
    pub otp_provider: String,
    pub allow_log_otp: bool,
    pub whatsapp_webhook_url: Option<String>,
    pub whatsapp_webhook_token: Option<String>,
    pub admob_keys_json: Option<String>,
    pub admob_keys_file: Option<String>,
    pub run_migrations: bool,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        let env = env::var("CH_ENV").unwrap_or_else(|_| "local".to_string());
        let jwt_secret =
            env::var("CH_JWT_SECRET").unwrap_or_else(|_| "dev-only-secret-change-me".to_string());
        if env != "local" && jwt_secret.starts_with("dev-only") {
            tracing::warn!(
                "CH_JWT_SECRET not set in a non-local environment — set it before deploy"
            );
        }
        let whatsapp_webhook_url = env::var("CH_WHATSAPP_WEBHOOK_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let whatsapp_webhook_token = env::var("CH_WHATSAPP_WEBHOOK_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty());
        let otp_provider = env::var("CH_OTP_PROVIDER").unwrap_or_else(|_| {
            if whatsapp_webhook_url.is_some() {
                "whatsapp".to_string()
            } else {
                "log".to_string()
            }
        });
        let allow_log_otp = env::var("CH_ALLOW_LOG_OTP")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let run_migrations = env::var("CH_RUN_MIGRATIONS")
            .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);
        Self {
            database_url: env::var("CH_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres@127.0.0.1:54329/hustle".to_string()),
            redis_url: env::var("CH_REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:63799/".to_string()),
            jwt_secret,
            env,
            host: env::var("CH_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: num_env("CH_PORT", 8080),
            version: env!("CARGO_PKG_VERSION"),
            welcome_gold: num_env("CH_ECONOMY_WELCOME_GOLD", 50_000),
            ads_gold_per_ad: num_env("CH_ADS_GOLD_PER_AD", 2_000),
            ads_max_per_day: num_env("CH_ADS_MAX_PER_DAY", 5),
            ads_cooldown_s: num_env("CH_ADS_COOLDOWN_S", 60),
            otp_provider,
            allow_log_otp,
            whatsapp_webhook_url,
            whatsapp_webhook_token,
            admob_keys_json: env::var("CH_ADMOB_KEYS_JSON").ok().filter(|s| !s.trim().is_empty()),
            admob_keys_file: env::var("CH_ADMOB_KEYS_FILE").ok().filter(|s| !s.trim().is_empty()),
            run_migrations,
        }
    }

    /// Enforce strict production safety invariants when `CH_ENV=prod` (or `production`).
    pub fn validate_for_production(&self) -> Result<(), String> {
        if self.env == "prod" || self.env == "production" {
            if self.jwt_secret.starts_with("dev-only") || self.jwt_secret.len() < 32 {
                return Err(
                    "CH_JWT_SECRET must be at least 32 characters and non-default in production"
                        .to_string(),
                );
            }
            if self.otp_provider == "log" && !self.allow_log_otp {
                return Err(
                    "CH_OTP_PROVIDER=log is forbidden in production unless CH_ALLOW_LOG_OTP=true"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
}

fn num_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_validation_rejects_dev_secret_and_log_otp() {
        let mut cfg = ServerConfig::from_env();
        cfg.env = "prod".to_string();
        cfg.jwt_secret = "dev-only-secret-change-me".to_string();
        assert!(cfg.validate_for_production().is_err());

        cfg.jwt_secret = "a-real-32-plus-character-production-jwt-secret".to_string();
        cfg.otp_provider = "log".to_string();
        cfg.allow_log_otp = false;
        assert!(cfg.validate_for_production().is_err());

        cfg.otp_provider = "whatsapp".to_string();
        assert!(cfg.validate_for_production().is_ok());
    }
}
