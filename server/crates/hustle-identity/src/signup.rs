//! T1 signup (schema §14): users + auth_identities + consent_records +
//! profiles + user_settings + welcome Gold grant — ONE atomic transaction.
//! Re-verification of an existing identity reuses the user (welcome grant is
//! idem-deduped by key `welcome:{identity}`).

use hustle_gold::{GoldReason, GrantResult, LedgerOp, PgGoldLedger};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;

use crate::sessions::SsoUser;
use crate::tokens::TokenService;

#[derive(Debug, Error)]
pub enum SignupError {
    #[error("age 18+ declaration missing or refused")]
    AgeGate,
    #[error("handle taken or invalid (3-20 chars, a-z0-9_)")]
    HandleTaken,
    #[error("display name must be 2-30 chars")]
    BadDisplayName,
    #[error("consent required for essential purposes")]
    ConsentMissing,
    #[error("phone invalid")]
    BadPhone,
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("ledger error")]
    Ledger(#[from] hustle_gold::LedgerError),
    #[error("session error")]
    Session(#[from] crate::sessions::SessionError),
    #[error("token error")]
    Token(#[from] crate::tokens::TokenError),
}

/// DPDP consent record set (IT Rules 2021 + ADR-010).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Consents {
    #[serde(default = "default_true")]
    pub essential: bool,
    #[serde(default)]
    pub functional: bool,
    #[serde(default)]
    pub marketing: bool,
    #[serde(default)]
    pub personalised_ads: bool,
    #[serde(default)]
    pub chat: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileInput {
    pub handle: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug)]
pub struct SignupOutcome {
    pub user_id: i64,
    pub session_id: i64,
    pub access_token: String,
    pub refresh_token: String,
    pub gold_balance: i64,
    pub handle: String,
    pub is_new_user: bool,
}

/// Validated public identity to bind at signup (phone after OTP, or Google sub).
pub enum VerifiedIdentity {
    Phone(String),
    Google(SsoUser),
}

pub const POLICY_VERSION: &str = "2026-11-v1";

pub struct SignupService {
    pool: PgPool,
    tokens: TokenService,
    welcome_gold: i64,
}

impl SignupService {
    pub fn new(pool: PgPool, tokens: TokenService, welcome_gold: i64) -> Self {
        Self {
            pool,
            tokens,
            welcome_gold,
        }
    }

    fn valid_handle(handle: &str) -> bool {
        handle.len() >= 3
            && handle.len() <= 20
            && handle
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    /// Full signup/verification. `ip_hash` for session + consent minimization.
    pub async fn signup(
        &self,
        identity: VerifiedIdentity,
        consents: Consents,
        profile: ProfileInput,
        device_label: Option<&str>,
        ip_hash: Option<&str>,
    ) -> Result<SignupOutcome, SignupError> {
        if !consents.essential {
            return Err(SignupError::ConsentMissing);
        }
        if !Self::valid_handle(&profile.handle) {
            return Err(SignupError::HandleTaken); // same code path for invalid
        }
        let display = profile.display_name.trim();
        if display.chars().count() < 2 || display.chars().count() > 30 {
            return Err(SignupError::BadDisplayName);
        }

        let mut tx = self.pool.begin().await?;

        // 1) user (idempotent on identity conflict; `inserted` tells us new).
        // `public_id` is the only id serialized to clients (rest.md §1).
        let (user_id, mut is_new_user) = match &identity {
            VerifiedIdentity::Phone(phone) => {
                if !crate::is_e164(phone) {
                    return Err(SignupError::BadPhone);
                }
                sqlx::query_as::<_, (i64, i32)>(
                    "WITH u AS (
                       INSERT INTO users (phone_e164, age_confirmed_at, public_id)
                       VALUES ($1, now(), $2)
                       ON CONFLICT (phone_e164) DO UPDATE SET age_confirmed_at = now()
                       RETURNING id, (xmax = 0)::int AS inserted
                     ) SELECT id, inserted FROM u",
                )
                .bind(phone)
                .bind(ulid::Ulid::new().to_string())
                .fetch_one(&mut *tx)
                .await
                .map(|(id, inserted)| (id, inserted == 1))?
            }
            VerifiedIdentity::Google(sso) => sqlx::query_as::<_, (i64, i32)>(
                "WITH u AS (
                       INSERT INTO users (email, age_confirmed_at, public_id)
                       VALUES ($1, now(), $2)
                       ON CONFLICT (email) DO UPDATE SET age_confirmed_at = now()
                       RETURNING id, (xmax = 0)::int AS inserted
                     ) SELECT id, inserted FROM u",
            )
            .bind(&sso.email)
            .bind(ulid::Ulid::new().to_string())
            .fetch_one(&mut *tx)
            .await
            .map(|(id, inserted)| (id, inserted == 1))?,
        };

        // 1b) auth identity row
        match &identity {
            VerifiedIdentity::Phone(phone) => {
                sqlx::query(
                    "INSERT INTO auth_identities (user_id, kind, identifier, verified_at)
                     VALUES ($1, 'phone', $2, now())
                     ON CONFLICT (kind, identifier) DO NOTHING",
                )
                .bind(user_id)
                .bind(phone)
                .execute(&mut *tx)
                .await?;
            }
            VerifiedIdentity::Google(sso) => {
                sqlx::query(
                    "INSERT INTO auth_identities (user_id, kind, identifier, verified_at)
                     VALUES ($1, 'google', $2, now())
                     ON CONFLICT (kind, identifier) DO NOTHING",
                )
                .bind(user_id)
                .bind(&sso.sub)
                .execute(&mut *tx)
                .await?;
            }
        }

        // 2) consent records (append-only, DPDP audit trail)
        for (purpose, granted) in [
            ("essential", consents.essential),
            ("functional", consents.functional),
            ("marketing", consents.marketing),
            ("personalised_ads", consents.personalised_ads),
            ("chat", consents.chat),
        ] {
            sqlx::query(
                "INSERT INTO consent_records (user_id, policy_version, purpose, granted, ip_hash)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(user_id)
            .bind(POLICY_VERSION)
            .bind(purpose)
            .bind(granted)
            .bind(ip_hash)
            .execute(&mut *tx)
            .await?;
        }

        // 3) profile — unique lower(handle) violation from ANOTHER user rejects
        match sqlx::query_scalar::<_, String>(
            "INSERT INTO profiles (user_id, handle, display_name)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id) DO NOTHING
             RETURNING handle",
        )
        .bind(user_id)
        .bind(&profile.handle)
        .bind(display)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(row) => {
                if row.is_none() {
                    is_new_user = false;
                }
            }
            Err(e)
                if e.as_database_error()
                    .map(|d| d.is_unique_violation())
                    .unwrap_or(false) =>
            {
                return Err(SignupError::HandleTaken);
            }
            Err(e) => return Err(e.into()),
        }

        sqlx::query(
            "INSERT INTO user_settings (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

        // 4) welcome grant (idem: welcome:{identity}) — replay-safe. The ledger
        // composes into THIS transaction (associated fn per ports pattern).
        let idem = match &identity {
            VerifiedIdentity::Phone(p) => format!("welcome:phone:{p}"),
            VerifiedIdentity::Google(s) => format!("welcome:google:{}", s.sub),
        };
        let op = LedgerOp {
            user_id,
            delta: self.welcome_gold,
            reason: GoldReason::Welcome,
            ref_type: None,
            ref_id: None,
            idem_key: Some(idem),
            meta: serde_json::json!({}),
        };
        let balance_after = match PgGoldLedger::apply_in_tx(&mut tx, &op).await? {
            GrantResult::Applied { balance_after, .. }
            | GrantResult::Duplicate { balance_after } => balance_after,
        };

        // 5) session in the same transaction — signup without a session is useless
        let issued =
            crate::sessions::SessionStore::create(&mut tx, user_id, device_label, ip_hash).await?;
        tx.commit().await?;

        let access = self
            .tokens
            .issue_access(user_id, "user", issued.session_id)?;

        Ok(SignupOutcome {
            user_id,
            session_id: issued.session_id,
            access_token: access,
            refresh_token: issued.refresh.raw,
            gold_balance: balance_after,
            handle: profile.handle,
            is_new_user,
        })
    }
}

pub async fn handle_taken(pool: &PgPool, handle: &str) -> Result<bool, sqlx::Error> {
    let row: Option<i64> =
        sqlx::query_scalar("SELECT user_id FROM profiles WHERE lower(handle) = lower($1)")
            .bind(handle)
            .fetch_optional(pool)
            .await?;
    Ok(row.is_some())
}

pub fn sha256_hex(input: &[u8]) -> String {
    sha2::Sha256::digest(input)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub type Txn<'c> = Transaction<'c, Postgres>;
