use crate::domain::{AdReject, RawCallback, VerifiedReward};
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::RwLock;
use time::OffsetDateTime;

pub trait AdVerifier: Send + Sync {
    fn verify(&self, raw: &RawCallback) -> Result<VerifiedReward, AdReject>;
}

/// Google AdMob SSV verifier using NIST P-256 (secp256r1) with SHA-256.
pub struct AdMobVerifier {
    keys: RwLock<HashMap<String, VerifyingKey>>,
}

impl Default for AdMobVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl AdMobVerifier {
    pub fn new() -> Self {
        Self {
            keys: RwLock::new(HashMap::new()),
        }
    }

    /// Register a verifying key by key_id.
    pub fn register_key(&self, key_id: &str, key: VerifyingKey) {
        let mut keys = self.keys.write().unwrap();
        keys.insert(key_id.to_string(), key);
    }

    /// Register a verifying key from base64 DER or SEC1 format.
    pub fn register_key_base64(&self, key_id: &str, b64: &str) -> Result<(), String> {
        let clean = b64.trim().replace('\n', "");
        let bytes = STANDARD
            .decode(&clean)
            .or_else(|_| URL_SAFE.decode(&clean))
            .or_else(|_| URL_SAFE_NO_PAD.decode(&clean))
            .map_err(|e| format!("invalid base64 key: {e}"))?;

        let key = VerifyingKey::from_public_key_der(&bytes)
            .or_else(|_| VerifyingKey::from_sec1_bytes(&bytes))
            .map_err(|e| format!("invalid public key bytes: {e}"))?;

        self.register_key(key_id, key);
        Ok(())
    }
}

impl AdVerifier for AdMobVerifier {
    fn verify(&self, raw: &RawCallback) -> Result<VerifiedReward, AdReject> {
        let key_id = raw
            .params
            .get("key_id")
            .ok_or_else(|| AdReject::MalformedCallback("missing key_id".into()))?;

        let verifying_key = {
            let keys = self.keys.read().unwrap();
            keys.get(key_id)
                .cloned()
                .ok_or_else(|| AdReject::UnknownKeyId(key_id.clone()))?
        };

        let sig_b64 = raw
            .params
            .get("signature")
            .ok_or_else(|| AdReject::MalformedCallback("missing signature".into()))?;

        let sig_b64_clean = sig_b64.replace(' ', "+");
        let sig_bytes = STANDARD
            .decode(&sig_b64_clean)
            .or_else(|_| URL_SAFE.decode(sig_b64))
            .or_else(|_| URL_SAFE_NO_PAD.decode(sig_b64))
            .map_err(|_| AdReject::InvalidSignature)?;

        let signature = Signature::from_der(&sig_bytes)
            .or_else(|_| Signature::from_slice(&sig_bytes))
            .map_err(|_| AdReject::InvalidSignature)?;

        // Extract the message signed by AdMob:
        // Google AdMob signs the query parameters string preceding the signature parameter:
        // e.g. "ad_network=...&custom_data=...&user_id=..." up to "&signature="
        let message_to_verify = if let Some(idx) = raw.query_string.find("&signature=") {
            &raw.query_string[..idx]
        } else if let Some(idx) = raw.query_string.find("signature=") {
            if let Some(amp_idx) = raw.query_string[idx..].find('&') {
                &raw.query_string[idx + amp_idx + 1..]
            } else {
                return Err(AdReject::MalformedCallback("query string has only signature".into()));
            }
        } else {
            return Err(AdReject::MalformedCallback("missing signature in query string".into()));
        };

        verifying_key
            .verify(message_to_verify.as_bytes(), &signature)
            .map_err(|_| AdReject::InvalidSignature)?;

        // Validate freshness (timestamp is in ms, older than 7 days rejected)
        let timestamp_str = raw
            .params
            .get("timestamp")
            .ok_or_else(|| AdReject::MalformedCallback("missing timestamp".into()))?;
        let timestamp_ms: i64 = timestamp_str
            .parse()
            .map_err(|_| AdReject::MalformedCallback("invalid timestamp format".into()))?;

        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let seven_days_ms = 7 * 86_400 * 1000;

        if timestamp_ms < (now_ms - seven_days_ms) {
            return Err(AdReject::ExpiredTimestamp(format!(
                "timestamp {timestamp_ms} is older than 7 days (now {now_ms})"
            )));
        }

        let custom_data = raw
            .params
            .get("custom_data")
            .ok_or_else(|| AdReject::MalformedCallback("missing custom_data (session_id)".into()))?;

        let user_id_str = raw
            .params
            .get("user_id")
            .ok_or_else(|| AdReject::MalformedCallback("missing user_id".into()))?;
        let user_id: i64 = user_id_str
            .parse()
            .map_err(|_| AdReject::MalformedCallback("invalid user_id".into()))?;

        let transaction_id = raw
            .params
            .get("transaction_id")
            .cloned()
            .unwrap_or_else(|| format!("tx_{}", custom_data));

        let mut hasher = Sha256::new();
        hasher.update(raw.query_string.as_bytes());
        let digest_bytes = hasher.finalize();
        let mut raw_digest = [0u8; 32];
        raw_digest.copy_from_slice(&digest_bytes);

        Ok(VerifiedReward {
            provider: "admob".to_string(),
            transaction_id,
            user_id,
            session_id: custom_data.clone(),
            raw_digest,
        })
    }
}
