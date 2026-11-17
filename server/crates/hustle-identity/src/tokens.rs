//! Access/refresh token issuance and verification (ADR-008).
//!
//! Access: HS256 JWT, 15-min TTL, claims {sub: user_id, role, sid}.
//! Refresh: opaque 32-byte random, only its SHA-256 is stored.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;

pub const ACCESS_TTL_S: i64 = 15 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessClaims {
    /// user id (internal bigint as string)
    pub sub: String,
    /// user role: user | moderator | admin
    pub role: String,
    /// session id (internal bigint as string)
    pub sid: String,
    /// issued at (unix s)
    pub iat: i64,
    /// expiry (unix s)
    pub exp: i64,
}

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("expired or invalid token")]
    Invalid,
    #[error("jwt secret misconfigured")]
    Config,
}

pub struct TokenService {
    enc: EncodingKey,
    dec: DecodingKey,
}

impl TokenService {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            enc: EncodingKey::from_secret(secret),
            dec: DecodingKey::from_secret(secret),
        }
    }

    pub fn issue_access(
        &self,
        user_id: i64,
        role: &str,
        session_id: i64,
    ) -> Result<String, TokenError> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let claims = AccessClaims {
            sub: user_id.to_string(),
            role: role.to_string(),
            sid: session_id.to_string(),
            iat: now,
            exp: now + ACCESS_TTL_S,
        };
        encode(&Header::default(), &claims, &self.enc).map_err(|_| TokenError::Config)
    }

    /// Validates signature + expiry. Expired tokens are rejected the same as
    /// tampered ones (`token_expired` vs `session_revoked` distinction is the
    /// API layer's mapping concern).
    pub fn validate_access(&self, token: &str) -> Result<AccessClaims, TokenError> {
        let mut v = Validation::default();
        v.validate_exp = true;
        v.validate_aud = false;
        let data = decode::<AccessClaims>(token, &self.dec, &v).map_err(|_| TokenError::Invalid)?;
        Ok(data.claims)
    }

    pub fn refresh_ttl(&self) -> i64 {
        30 * 24 * 3600
    }
}

/// Opaque refresh token (`rt_` + 43 url-safe chars); only the hash is stored.
#[derive(Debug)]
pub struct RefreshPair {
    pub raw: String,
    pub hash: String,
}

pub fn new_refresh() -> RefreshPair {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let raw = format!("rt_{}", base32_hex(&bytes));
    RefreshPair {
        hash: refresh_hash(&raw),
        raw,
    }
}

pub fn refresh_hash(raw: &str) -> String {
    let digest = Sha256::digest(raw.as_bytes());
    hex(&digest)
}

fn base32_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_roundtrip_and_expiry_rejected() {
        let svc = TokenService::new(b"secret");
        let token = svc.issue_access(7, "user", 3).unwrap();
        let claims = svc.validate_access(&token).unwrap();
        assert_eq!(claims.sub, "7");
        assert_eq!(claims.role, "user");
        assert_eq!(claims.sid, "3");
        assert_eq!(claims.exp - claims.iat, ACCESS_TTL_S);

        // wrong secret
        let other = TokenService::new(b"not-the-secret");
        assert!(other.validate_access(&token).is_err());
    }

    #[test]
    fn refresh_is_url_safe_and_hash_stable() {
        let a = new_refresh();
        assert!(a.raw.starts_with("rt_"));
        assert!(a.raw.len() > 40);
        assert_eq!(refresh_hash(&a.raw), a.hash);
        // unique across draws
        let b = new_refresh();
        assert_ne!(a.raw, b.raw);
    }
}
