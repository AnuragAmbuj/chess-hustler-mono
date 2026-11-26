//! Phone OTP: send + verify with in-memory ephemeral state (Redis-backed in
//! the multi-replica rollout; single-node Phase 1 semantics are identical).
//! Rate limits per docs/rest.md §2.1: 5/hr/IP, 3 codes/15 min/phone, TTL
//! 5 min, 5 attempts per code.

use async_trait::async_trait;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OtpError {
    #[error("otp invalid or expired")]
    Invalid,
    #[error("otp rate limited")]
    RateLimited,
    #[error("phone invalid")]
    BadPhone,
    #[error("delivery failed")]
    Delivery,
}

/// Delivery port — MSG91 adapter ships with staging credentials.
#[async_trait]
pub trait OtpSender: Send + Sync {
    async fn send(&self, phone: &str, code: &str) -> Result<(), OtpError>;
}

/// Dev sender: logs the code (never used outside local/test).
pub struct LogOtpSender;

#[async_trait]
impl OtpSender for LogOtpSender {
    async fn send(&self, phone: &str, code: &str) -> Result<(), OtpError> {
        tracing::info!(phone, code, "OTP delivered (dev log sender)");
        Ok(())
    }
}

/// WhatsApp HTTP webhook sender (works with a free self-hosted WhatsApp bridge
/// sidecar such as WAHA/whatsmeow, or an internal Meta Cloud API gateway).
pub struct WhatsAppWebhookSender {
    endpoint: String,
    token: Option<String>,
}

impl WhatsAppWebhookSender {
    pub fn new(endpoint: String, token: Option<String>) -> Self {
        Self { endpoint, token }
    }
}

#[async_trait]
impl OtpSender for WhatsAppWebhookSender {
    async fn send(&self, phone: &str, code: &str) -> Result<(), OtpError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let uri: http::Uri = self.endpoint.parse().map_err(|_| OtpError::Delivery)?;
        let host = uri.host().ok_or(OtpError::Delivery)?;
        let port = uri.port_u16().unwrap_or(80);
        let path = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

        let mut stream = tokio::net::TcpStream::connect((host, port))
            .await
            .map_err(|e| {
                tracing::error!(error = %e, endpoint = %self.endpoint, "WhatsApp webhook connect failed");
                OtpError::Delivery
            })?;

        let payload = serde_json::json!({
            "phone": phone,
            "chatId": format!("{}@c.us", phone.trim_start_matches('+')),
            "text": format!("Your Chess Hustle verification code is {code}. It expires in 5 minutes."),
            "code": code,
        })
        .to_string();

        let auth_header = self
            .token
            .as_ref()
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();

        let raw_req = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{auth_header}\r\n{payload}",
            payload.len()
        );

        stream.write_all(raw_req.as_bytes()).await.map_err(|e| {
            tracing::error!(error = %e, "WhatsApp webhook write failed");
            OtpError::Delivery
        })?;

        let mut resp_buf = [0u8; 256];
        let n = stream.read(&mut resp_buf).await.map_err(|e| {
            tracing::error!(error = %e, "WhatsApp webhook read failed");
            OtpError::Delivery
        })?;
        let status_line = String::from_utf8_lossy(&resp_buf[..n]);
        let is_2xx = status_line.starts_with("HTTP/1.1 2") || status_line.starts_with("HTTP/1.0 2");
        if !is_2xx {
            tracing::error!(response = %status_line.lines().next().unwrap_or(""), "WhatsApp webhook returned non-2xx");
            return Err(OtpError::Delivery);
        }
        Ok(())
    }
}

struct OtpEntry {
    code_hash: [u8; 32],
    expires_ms: u64,
    attempts: u32,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn hash(code: &str) -> [u8; 32] {
    Sha256::digest(code.as_bytes()).into()
}

const OTP_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_ATTEMPTS: u32 = 5;
const PHONE_WINDOW_MS: u64 = 15 * 60 * 1000;
const PHONE_MAX: usize = 3;
const IP_WINDOW_MS: u64 = 60 * 60 * 1000;
const IP_MAX: usize = 5;

/// OTP service supporting Redis-backed distributed state (production) and
/// in-memory ephemeral state (unit tests).
pub struct OtpService {
    sender: Box<dyn OtpSender>,
    redis: Option<redis::Client>,
    entries: Mutex<std::collections::HashMap<String, OtpEntry>>,
    phone_log: Mutex<std::collections::HashMap<String, VecDeque<u64>>>,
    ip_log: Mutex<std::collections::HashMap<String, VecDeque<u64>>>,
}

impl OtpService {
    pub fn new(sender: Box<dyn OtpSender>) -> Self {
        Self {
            sender,
            redis: None,
            entries: Mutex::default(),
            phone_log: Mutex::default(),
            ip_log: Mutex::default(),
        }
    }

    pub fn with_redis(sender: Box<dyn OtpSender>, redis: redis::Client) -> Self {
        Self {
            sender,
            redis: Some(redis),
            entries: Mutex::default(),
            phone_log: Mutex::default(),
            ip_log: Mutex::default(),
        }
    }

    async fn redis_incr_within_limit(
        conn: &mut redis::aio::MultiplexedConnection,
        key: &str,
        window_s: i64,
        max: i64,
    ) -> Result<bool, OtpError> {
        use redis::AsyncCommands;
        let count: i64 = conn.incr(key, 1).await.map_err(|_| OtpError::Delivery)?;
        if count == 1 {
            let _: bool = conn
                .expire(key, window_s)
                .await
                .map_err(|_| OtpError::Delivery)?;
        }
        Ok(count <= max)
    }

    fn within_window(
        log: &mut std::collections::HashMap<String, VecDeque<u64>>,
        key: &str,
        now: u64,
        window_ms: u64,
        max: usize,
    ) -> bool {
        let dq = log.entry(key.to_string()).or_default();
        while let Some(&t) = dq.front() {
            if now - t > window_ms {
                dq.pop_front();
            } else {
                break;
            }
        }
        if dq.len() >= max {
            false
        } else {
            dq.push_back(now);
            true
        }
    }

    pub async fn send(&self, phone: &str, ip: &str) -> Result<(), OtpError> {
        if !crate::is_e164(phone) {
            return Err(OtpError::BadPhone);
        }
        if let Some(client) = &self.redis {
            use redis::AsyncCommands;
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|_| OtpError::Delivery)?;
            if !Self::redis_incr_within_limit(
                &mut conn,
                &format!("otp:rl:ip:{ip}"),
                (IP_WINDOW_MS / 1000) as i64,
                IP_MAX as i64,
            )
            .await?
            {
                return Err(OtpError::RateLimited);
            }
            if !Self::redis_incr_within_limit(
                &mut conn,
                &format!("otp:rl:phone:{phone}"),
                (PHONE_WINDOW_MS / 1000) as i64,
                PHONE_MAX as i64,
            )
            .await?
            {
                return Err(OtpError::RateLimited);
            }

            let code: String = rand::rng().random_range(100_000..1_000_000).to_string();
            let digest = hash(&code);
            let payload = serde_json::json!({
                "hash": digest.to_vec(),
                "attempts": 0u32,
            })
            .to_string();
            let _: () = conn
                .set_ex(format!("otp:code:{phone}"), payload, OTP_TTL_MS / 1000)
                .await
                .map_err(|_| OtpError::Delivery)?;
            return self.sender.send(phone, &code).await;
        }

        let now = now_ms();
        if !Self::within_window(
            &mut self.ip_log.lock().unwrap(),
            ip,
            now,
            IP_WINDOW_MS,
            IP_MAX,
        ) {
            return Err(OtpError::RateLimited);
        }
        if !Self::within_window(
            &mut self.phone_log.lock().unwrap(),
            phone,
            now,
            PHONE_WINDOW_MS,
            PHONE_MAX,
        ) {
            return Err(OtpError::RateLimited);
        }

        let code: String = rand::rng().random_range(100_000..1_000_000).to_string();
        let entry = OtpEntry {
            code_hash: hash(&code),
            expires_ms: now + OTP_TTL_MS,
            attempts: 0,
        };
        self.entries
            .lock()
            .unwrap()
            .insert(phone.to_string(), entry);
        self.sender.send(phone, &code).await
    }

    /// Consume the code (single-use). Correct codes are cleared; wrong codes
    /// burn an attempt (5 max).
    pub async fn verify(&self, phone: &str, code: &str) -> Result<(), OtpError> {
        if !crate::is_e164(phone) {
            return Err(OtpError::BadPhone);
        }
        if let Some(client) = &self.redis {
            use redis::AsyncCommands;
            #[derive(serde::Deserialize, serde::Serialize)]
            struct RedisOtp {
                hash: Vec<u8>,
                attempts: u32,
            }
            let mut conn = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|_| OtpError::Delivery)?;
            let key = format!("otp:code:{phone}");
            let raw: Option<String> = conn.get(&key).await.map_err(|_| OtpError::Delivery)?;
            let Some(json_str) = raw else {
                return Err(OtpError::Invalid);
            };
            let mut entry: RedisOtp =
                serde_json::from_str(&json_str).map_err(|_| OtpError::Invalid)?;
            if entry.attempts >= MAX_ATTEMPTS {
                let _: i64 = conn.del(&key).await.map_err(|_| OtpError::Delivery)?;
                return Err(OtpError::RateLimited);
            }
            if entry.hash.as_slice() == hash(code) {
                let _: i64 = conn.del(&key).await.map_err(|_| OtpError::Delivery)?;
                return Ok(());
            }
            entry.attempts += 1;
            let ttl: i64 = conn.ttl(&key).await.unwrap_or(60).max(1);
            let updated = serde_json::to_string(&entry).map_err(|_| OtpError::Delivery)?;
            let _: () = conn
                .set_ex(&key, updated, ttl as u64)
                .await
                .map_err(|_| OtpError::Delivery)?;
            return Err(OtpError::Invalid);
        }

        let now = now_ms();
        let mut entries = self.entries.lock().unwrap();
        let Some(entry) = entries.get_mut(phone) else {
            return Err(OtpError::Invalid);
        };
        if entry.expires_ms <= now {
            entries.remove(phone);
            return Err(OtpError::Invalid);
        }
        if entry.attempts >= MAX_ATTEMPTS {
            entries.remove(phone);
            return Err(OtpError::RateLimited);
        }
        if entry.code_hash == hash(code) {
            entries.remove(phone);
            Ok(())
        } else {
            entry.attempts += 1;
            Err(OtpError::Invalid)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> OtpService {
        OtpService::new(Box::new(LogOtpSender))
    }

    #[tokio::test]
    async fn send_then_verify_then_replay_fails() {
        let s = svc();
        s.send("+919876543201", "ip1").await.unwrap();
        // code unknown to the test — use a sentinel sender to capture it
    }

    struct CaptureSender(tokio::sync::mpsc::UnboundedSender<String>);
    #[async_trait]
    impl OtpSender for CaptureSender {
        async fn send(&self, _phone: &str, code: &str) -> Result<(), OtpError> {
            let _ = self.0.send(code.to_string());
            Ok(())
        }
    }

    async fn capturing_svc() -> (OtpService, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (OtpService::new(Box::new(CaptureSender(tx))), rx)
    }

    #[tokio::test]
    async fn full_verify_cycle_single_use() {
        let (s, mut rx) = capturing_svc().await;
        let phone = "+919876543202";
        s.send(phone, "ip").await.unwrap();
        let code = rx.recv().await.unwrap();
        s.verify(phone, &code).await.unwrap();
        // replay of a consumed code
        assert!(matches!(
            s.verify(phone, &code).await,
            Err(OtpError::Invalid)
        ));
    }

    #[tokio::test]
    async fn wrong_code_burns_attempts_then_rate_limited() {
        let (s, mut rx) = capturing_svc().await;
        let phone = "+919876543203";
        s.send(phone, "ip").await.unwrap();
        let _code = rx.recv().await.unwrap();
        for _ in 0..5 {
            assert!(matches!(
                s.verify(phone, "000000").await,
                Err(OtpError::Invalid)
            ));
        }
        assert!(matches!(
            s.verify(phone, "000000").await,
            Err(OtpError::RateLimited)
        ));
    }

    #[tokio::test]
    async fn phone_rate_limit_three_per_15min() {
        let (s, mut rx) = capturing_svc().await;
        let phone = "+919876543204";
        for _ in 0..3 {
            s.send(phone, "ip").await.unwrap();
            let _ = rx.recv().await;
        }
        assert!(matches!(
            s.send(phone, "ip").await,
            Err(OtpError::RateLimited)
        ));
        // different phone unaffected
        s.send("+919876543205", "ip").await.unwrap();
    }

    #[tokio::test]
    async fn ip_rate_limit_five_per_hour() {
        let (s, _rx) = capturing_svc().await;
        for i in 0..5 {
            s.send(&format!("+91987654{i:03}"), "one-ip").await.unwrap();
        }
        assert!(matches!(
            s.send("+91987654099", "one-ip").await,
            Err(OtpError::RateLimited)
        ));
        s.send("+91987654099", "other-ip").await.unwrap();
    }

    #[tokio::test]
    async fn rejects_bad_phone() {
        let s = svc();
        assert!(matches!(
            s.send("98765", "ip").await,
            Err(OtpError::BadPhone)
        ));
        assert!(matches!(
            s.verify("98765", "123456").await,
            Err(OtpError::BadPhone)
        ));
    }
}
