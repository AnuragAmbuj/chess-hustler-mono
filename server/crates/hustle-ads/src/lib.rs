pub mod domain;
pub mod service;
pub mod verifier;

pub use domain::*;
pub use service::*;
pub use verifier::*;

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use time::OffsetDateTime;

    fn get_test_key(seed: u8) -> SigningKey {
        let mut key_bytes = [seed; 32];
        key_bytes[0] = seed.max(1);
        SigningKey::from_slice(&key_bytes).expect("valid scalar")
    }

    #[test]
    fn test_admob_verifier_valid_signature() {
        let signing_key = get_test_key(42);
        let verifying_key = signing_key.verifying_key();

        let verifier = AdMobVerifier::new();
        verifier.register_key("test_key_1", *verifying_key);

        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let base_qs = format!(
            "ad_network=test_network&custom_data=as_01JDMK123&reward_amount=2000&timestamp={}&transaction_id=tx_998877&user_id=12345&key_id=test_key_1",
            now_ms
        );

        // Sign the base query string
        let signature: p256::ecdsa::Signature = signing_key.sign(base_qs.as_bytes());
        let sig_der = signature.to_der();
        let sig_b64 = STANDARD.encode(sig_der.as_bytes());

        // Full callback query string with &signature=... at the end
        let full_qs = format!("{}&signature={}", base_qs, sig_b64);
        let callback = RawCallback::from_query_string(&full_qs);

        let result = verifier.verify(&callback);
        assert!(result.is_ok(), "Expected verification to succeed: {:?}", result.err());

        let verified = result.unwrap();
        assert_eq!(verified.provider, "admob");
        assert_eq!(verified.transaction_id, "tx_998877");
        assert_eq!(verified.session_id, "as_01JDMK123");
        assert_eq!(verified.user_id, 12345);
    }

    #[test]
    fn test_admob_verifier_tampered_payload_rejected() {
        let signing_key = get_test_key(77);
        let verifying_key = signing_key.verifying_key();

        let verifier = AdMobVerifier::new();
        verifier.register_key("test_key_1", *verifying_key);

        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let base_qs = format!(
            "custom_data=as_01JDMK123&reward_amount=2000&timestamp={}&transaction_id=tx_111&user_id=12345&key_id=test_key_1",
            now_ms
        );

        let signature: p256::ecdsa::Signature = signing_key.sign(base_qs.as_bytes());
        let sig_b64 = STANDARD.encode(signature.to_der().as_bytes());

        // Tamper user_id in the query string
        let tampered_qs = format!(
            "custom_data=as_01JDMK123&reward_amount=2000&timestamp={}&transaction_id=tx_111&user_id=99999&key_id=test_key_1&signature={}",
            now_ms, sig_b64
        );
        let callback = RawCallback::from_query_string(&tampered_qs);

        let result = verifier.verify(&callback);
        assert_eq!(result.unwrap_err(), AdReject::InvalidSignature);
    }

    #[test]
    fn test_admob_verifier_expired_timestamp_rejected() {
        let signing_key = get_test_key(88);
        let verifying_key = signing_key.verifying_key();

        let verifier = AdMobVerifier::new();
        verifier.register_key("test_key_1", *verifying_key);

        // 8 days ago
        let old_ms = (OffsetDateTime::now_utc().unix_timestamp() - (8 * 86_400)) * 1000;
        let base_qs = format!(
            "custom_data=as_01JDMK123&timestamp={}&transaction_id=tx_111&user_id=12345&key_id=test_key_1",
            old_ms
        );

        let signature: p256::ecdsa::Signature = signing_key.sign(base_qs.as_bytes());
        let sig_b64 = STANDARD.encode(signature.to_der().as_bytes());

        let full_qs = format!("{}&signature={}", base_qs, sig_b64);
        let callback = RawCallback::from_query_string(&full_qs);

        let result = verifier.verify(&callback);
        match result {
            Err(AdReject::ExpiredTimestamp(_)) => {}
            other => panic!("Expected ExpiredTimestamp error, got: {:?}", other),
        }
    }

    #[test]
    fn test_admob_verifier_unknown_key_id_rejected() {
        let verifier = AdMobVerifier::new();
        let callback = RawCallback::from_query_string("custom_data=as_1&key_id=nonexistent&signature=abcd&timestamp=1000&user_id=1");
        let result = verifier.verify(&callback);
        assert_eq!(result.unwrap_err(), AdReject::UnknownKeyId("nonexistent".into()));
    }

    #[test]
    fn test_admob_verifier_loads_keys_from_google_json() {
        use p256::pkcs8::EncodePublicKey;
        let signing_key = get_test_key(99);
        let der = signing_key
            .verifying_key()
            .to_public_key_der()
            .expect("der");
        let b64 = STANDARD.encode(der.as_bytes());
        let json = serde_json::json!({
            "keys": [
                {
                    "keyId": 3335741209u64,
                    "base64": b64
                }
            ]
        })
        .to_string();

        let verifier = AdMobVerifier::new();
        let count = verifier.load_from_json(&json).expect("load keys");
        assert_eq!(count, 1);

        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let base_qs = format!(
            "custom_data=as_json_1&timestamp={now_ms}&transaction_id=tx_json&user_id=77&key_id=3335741209"
        );
        let sig: p256::ecdsa::Signature = signing_key.sign(base_qs.as_bytes());
        let sig_b64 = STANDARD.encode(sig.to_der().as_bytes());
        let full_qs = format!("{base_qs}&signature={sig_b64}");

        let verified = verifier
            .verify(&RawCallback::from_query_string(&full_qs))
            .expect("signature from json key should verify");
        assert_eq!(verified.user_id, 77);
    }
}
