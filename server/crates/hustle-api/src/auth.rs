//! Auth routes (rest.md §2.1): OTP send/verify (signup), refresh rotation,
//! logout, logout-all. Public per spec; `Authed` guards the two logout routes.

use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use hustle_identity::sessions::SessionStore;
use hustle_identity::signup::{Consents, ProfileInput, SignupService, VerifiedIdentity};
use hustle_identity::OtpService;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::AppState;

/// `POST /v1/auth/otp/send` — 204 on success. Rate limits: 5/hr/IP,
/// 3/15min/phone (enforced in hustle-identity). Client IP comes from
/// `X-Forwarded-For` set by our edge; Phase 2 pins this to signed headers.
#[derive(Debug, Deserialize)]
pub struct OtpSendRequest {
    pub phone: String,
}

pub async fn otp_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OtpSendRequest>,
) -> Result<StatusCode, ApiError> {
    let ip = client_ip(&headers);
    state.otp.send(&req.phone, &ip).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/auth/otp/verify` — consumes the code, then runs T1 signup
/// atomically and returns the token pair + user payload.
#[derive(Debug, Deserialize)]
pub struct OtpVerifyRequest {
    pub phone: String,
    pub code: String,
    #[serde(default)]
    pub age_confirmed: bool,
    /// Missing consents object is a hard validation error (DPDP: consent is
    /// never defaulted to granted).
    pub consents: Option<Consents>,
    pub profile: ProfileInput,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

pub async fn otp_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OtpVerifyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !req.age_confirmed {
        return Err(ApiError::forbidden(
            "auth.age_gate",
            "18+ declaration missing or refused",
        ));
    }
    let consents = req
        .consents
        .ok_or_else(|| ApiError::validation("consents object required", json!(["consents"])))?;
    state.otp.verify(&req.phone, &req.code).await?;

    let ip = client_ip(&headers);
    let outcome = state
        .signup
        .signup(
            VerifiedIdentity::Phone(req.phone),
            consents,
            req.profile,
            None, // device label joins with push-token registration later
            Some(&crate::auth::hash_ip(&ip)),
        )
        .await?;

    let user = user_payload(&state, &outcome).await?;
    Ok(Json(json!({
        "access_token": outcome.access_token,
        "expires_in": hustle_identity::tokens::ACCESS_TTL_S,
        "refresh_token": outcome.refresh_token,
        "user": user,
    })))
}

/// `POST /v1/auth/refresh` — rotates the refresh token. Reuse of a rotated
/// token has ALREADY revoked the family when this returns 401.
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (issued, old) = state.sessions.rotate(&req.refresh_token).await?;
    let access = state
        .tokens
        .issue_access(issued.user_id, "user", issued.session_id)?;
    tracing::debug!(
        user = issued.user_id,
        rotated_from = old.session_id,
        "refresh rotated"
    );
    Ok(Json(json!({
        "access_token": access,
        "expires_in": hustle_identity::tokens::ACCESS_TTL_S,
        "refresh_token": issued.refresh.raw,
    })))
}

/// `POST /v1/auth/logout` — revokes the CURRENT session (the bearer's).
pub async fn logout(State(state): State<AppState>, authed: Authed) -> Result<StatusCode, ApiError> {
    state.sessions.revoke(authed.session_id, "logout").await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/auth/logout-all` — revokes EVERY session of the user.
pub async fn logout_all(
    State(state): State<AppState>,
    authed: Authed,
) -> Result<StatusCode, ApiError> {
    let n = state
        .sessions
        .revoke_all(authed.user_id, "logout_all")
        .await?;
    tracing::info!(user = authed.user_id, revoked = n, "logout-all");
    Ok(StatusCode::NO_CONTENT)
}

// ---- extractor -------------------------------------------------------------

/// Bearer-authenticated identity from the access JWT (15 min TTL).
#[derive(Debug, Clone)]
pub struct Authed {
    pub user_id: i64,
    pub role: String,
    pub session_id: i64,
}

impl FromRequestParts<AppState> for Authed {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let bearer = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
        let claims = state.tokens.validate_access(bearer)?;
        let user_id: i64 = claims
            .sub
            .parse()
            .map_err(|_| ApiError::unauthorized("malformed token subject"))?;
        let session_id: i64 = claims
            .sid
            .parse()
            .map_err(|_| ApiError::unauthorized("malformed token session"))?;
        Ok(Authed {
            user_id,
            role: claims.role,
            session_id,
        })
    }
}

/// Optional Bearer-authenticated identity for public endpoints that personalize when logged in.
#[derive(Debug, Clone)]
pub struct OptionalAuthed(pub Option<Authed>);

impl FromRequestParts<AppState> for OptionalAuthed {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let opt = (|| {
            let bearer = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)?
                .to_str()
                .ok()?
                .strip_prefix("Bearer ")?;
            let claims = state.tokens.validate_access(bearer).ok()?;
            let user_id: i64 = claims.sub.parse().ok()?;
            let session_id: i64 = claims.sid.parse().ok()?;
            Some(Authed {
                user_id,
                role: claims.role,
                session_id,
            })
        })();
        Ok(OptionalAuthed(opt))
    }
}

// ---- helpers ---------------------------------------------------------------

/// Edge-provided client IP (X-Forwarded-For first hop); logged IP is hashed
/// before storage (DPDP minimization). Spoofing is an edge concern in Phase 2.
fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("0.0.0.0")
        .to_string()
}

pub fn hash_ip(ip: &str) -> String {
    hustle_identity::sessions::hash_ip(ip)
}

/// `/auth/*` `user` payload: public_id (never the bigint), handle, display
/// name, gold, per-pool ratings with 1200 defaults.
async fn user_payload(
    state: &AppState,
    outcome: &hustle_identity::signup::SignupOutcome,
) -> Result<serde_json::Value, ApiError> {
    let public_id: Option<String> = sqlx::query_scalar("SELECT public_id FROM users WHERE id = $1")
        .bind(outcome.user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "public_id lookup failed");
            ApiError::internal("user payload failed")
        })?;

    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM profiles WHERE user_id = $1")
            .bind(outcome.user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "profile lookup failed");
                ApiError::internal("user payload failed")
            })?;

    let mut ratings = json!({
        "bullet": 1200, "blitz": 1200, "rapid": 1200, "classical": 1200,
    });
    let mut provisional = true;
    for r in hustle_ratings::get_all(&state.pool, outcome.user_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ratings lookup failed");
            ApiError::internal("user payload failed")
        })?
    {
        ratings[r.pool.as_str()] = json!(r.rating as i64);
        provisional &= r.games_count == 0;
    }

    Ok(json!({
        "id": public_id.unwrap_or_default(),
        "handle": outcome.handle,
        "display_name": display_name.unwrap_or_else(|| outcome.handle.clone()),
        "gold": outcome.gold_balance,
        "ratings": ratings,
        "provisional": provisional,
    }))
}

/// Kept for direct construction in tests that need only the store.
pub fn session_store(pool: sqlx::PgPool) -> SessionStore {
    SessionStore::new(pool)
}

/// SignupService stays constructible from api wiring (used by AppState).
pub fn signup_service(
    pool: sqlx::PgPool,
    tokens: hustle_identity::tokens::TokenService,
    welcome_gold: i64,
) -> SignupService {
    SignupService::new(pool, tokens, welcome_gold)
}

/// OtpService stays constructible from api wiring.
pub fn otp_service(sender: Box<dyn hustle_identity::OtpSender>) -> OtpService {
    OtpService::new(sender)
}
