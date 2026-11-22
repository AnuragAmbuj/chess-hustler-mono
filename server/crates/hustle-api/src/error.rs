//! REST error envelope (rest.md §1/§4): every failure is
//! `{ "error": { "code", "message", "details"? } }` with codes from the stable
//! registry. Domain errors map here — codes NEVER leak internals.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hustle_identity::sessions::SessionError;
use hustle_identity::signup::SignupError;
use hustle_identity::tokens::TokenError;
use hustle_identity::OtpError;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub details: Option<Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }

    pub fn validation(message: impl Into<String>, fields: Value) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "validation.failed", message)
            .with_details(json!({ "fields": fields }))
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "auth.token_expired", message)
    }

    pub fn session_revoked(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "auth.session_revoked", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut err = json!({
            "code": self.code,
            "message": self.message,
        });
        if let Some(details) = self.details {
            err["details"] = details;
        }
        (self.status, Json(json!({ "error": err }))).into_response()
    }
}

impl From<OtpError> for ApiError {
    fn from(e: OtpError) -> Self {
        match e {
            OtpError::Invalid => ApiError::new(
                StatusCode::UNAUTHORIZED,
                "auth.otp_invalid",
                "otp invalid or expired",
            ),
            OtpError::RateLimited => ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "auth.otp_rate_limited",
                "too many otp requests; retry later",
            ),
            OtpError::BadPhone => {
                ApiError::validation("phone must be a valid E.164 number", json!(["phone"]))
            }
            OtpError::Delivery => ApiError::internal("otp delivery failed"),
        }
    }
}

impl From<SignupError> for ApiError {
    fn from(e: SignupError) -> Self {
        match e {
            SignupError::AgeGate => {
                ApiError::forbidden("auth.age_gate", "18+ declaration missing or refused")
            }
            SignupError::HandleTaken => ApiError::new(
                StatusCode::CONFLICT,
                "conflict",
                "handle taken or invalid (3-20 chars, a-z0-9_)",
            ),
            SignupError::BadDisplayName => {
                ApiError::validation("display name must be 2-30 chars", json!(["display_name"]))
            }
            SignupError::ConsentMissing => ApiError::validation(
                "essential-consent is required to create an account",
                json!(["consents.essential"]),
            ),
            SignupError::BadPhone => {
                ApiError::validation("phone must be a valid E.164 number", json!(["phone"]))
            }
            SignupError::Db(e) => {
                tracing::error!(error = %e, "signup db error");
                ApiError::internal("signup failed")
            }
            SignupError::Ledger(e) => {
                tracing::error!(error = %e, "signup ledger error");
                ApiError::internal("signup failed")
            }
            SignupError::Session(e) => {
                tracing::error!(error = %e, "signup session error");
                ApiError::internal("signup failed")
            }
            SignupError::Token(e) => {
                tracing::error!(error = %e, "signup token error");
                ApiError::internal("signup failed")
            }
        }
    }
}

impl From<SessionError> for ApiError {
    fn from(e: SessionError) -> Self {
        match e {
            SessionError::Unknown | SessionError::Reused | SessionError::Revoked(_) => {
                ApiError::session_revoked(e.to_string())
            }
            SessionError::Db(e) => {
                tracing::error!(error = %e, "session db error");
                ApiError::internal("session operation failed")
            }
        }
    }
}

impl From<TokenError> for ApiError {
    fn from(e: TokenError) -> Self {
        match e {
            TokenError::Invalid => ApiError::unauthorized("token invalid or expired"),
            TokenError::Config => ApiError::internal("token configuration error"),
        }
    }
}

impl From<hustle_gold::LedgerError> for ApiError {
    fn from(e: hustle_gold::LedgerError) -> Self {
        match e {
            hustle_gold::LedgerError::Insufficient { balance, needed } => ApiError::new(
                StatusCode::PAYMENT_REQUIRED,
                "economy.insufficient_gold",
                format!("insufficient gold: have {balance}, need {needed}"),
            ),
            hustle_gold::LedgerError::BadDelta { delta, reason } => ApiError::validation(
                format!("invalid delta {delta} for reason {reason}"),
                json!(["delta"]),
            ),
            hustle_gold::LedgerError::Db(e) => {
                tracing::error!(error = %e, "database error in ledger operation");
                ApiError::internal("ledger operation failed")
            }
        }
    }
}

impl From<hustle_matchmaking::MatchmakingError> for ApiError {
    fn from(e: hustle_matchmaking::MatchmakingError) -> Self {
        match e {
            hustle_matchmaking::MatchmakingError::UserNotFound(id) => {
                ApiError::not_found("user.not_found", format!("user {id} not found"))
            }
            other => {
                tracing::error!(error = %other, "matchmaking error");
                ApiError::internal("matchmaking operation failed")
            }
        }
    }
}
