//! hustle-identity — auth, sessions, consent (ADR-008, ADR-010).
//!
//! Owns `users`, `auth_identities`, `user_sessions`, `consent_records`.
//! Cross-domain Gold (welcome grant) goes through the `GoldLedger` port —
//! composed INSIDE the T1 signup transaction (schema §14).
//!
//! Phase 1 transports: phone OTP (dev logging sender behind the `OtpSender`
//! port; MSG91 adapter ships with staging credentials) and Google id_token
//! (behind `GoogleVerifier`). Sessions: 15-min access JWT + rotating refresh
//! token with family reuse detection.

pub mod otp;
pub mod sessions;
pub mod signup;
pub mod tokens;

pub use otp::{LogOtpSender, OtpError, OtpSender, OtpService, WhatsAppWebhookSender};
pub use sessions::{SessionError, SessionStore, SsoUser};
pub use signup::{
    Consents, ProfileInput, SignupError, SignupOutcome, SignupService, VerifiedIdentity,
};
pub use tokens::{AccessClaims, TokenError, TokenService};

/// E.164 sanity: `+[1-9][0-9]{7,14}` (server never trusts formatting).
pub fn is_e164(phone: &str) -> bool {
    let Some(rest) = phone.strip_prefix('+') else {
        return false;
    };
    let digits = rest.chars().all(|c| c.is_ascii_digit());
    digits && (8..=15).contains(&rest.len())
}

#[cfg(test)]
mod tests {
    use super::is_e164;

    #[test]
    fn e164_validation() {
        assert!(is_e164("+919876543210"));
        assert!(is_e164("+12025550123"));
        assert!(!is_e164("9876543210"));
        assert!(!is_e164("+91"));
        assert!(!is_e164("+9198765432101234")); // 16 digits → too long
        assert!(!is_e164("+91987654321o"));
    }
}
