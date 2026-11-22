use crate::anticheat::AntiCheatAnalyzer;
use crate::error::ModerationError;
use crate::models::{
    ActionReportRequest, AdminAuditItem, AdminPlayerDetail, AnomalyFlagItem, ChatMessageItem,
    CreateReportRequest, CreateSanctionRequest, GrievanceInfo, GrievancePolicyLink, ReportItem,
    ResolveFlagRequest, SanctionItem,
};
use hustle_realtime::EventPublisher;
use redis::AsyncCommands;
use serde_json::json;
use sqlx::{PgPool, Row};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const PROFANITY_LIST: &[&str] = &[
    "chutiya",
    "bhenchod",
    "madarchod",
    "gandu",
    "bhosdike",
    "harami",
    "asshole",
    "bitch",
    "bastard",
    "fuck",
    "shit",
    "nigger",
    "faggot",
];

#[derive(Clone)]
pub struct ModerationService {
    pool: PgPool,
    redis: redis::Client,
    publisher: EventPublisher,
    analyzer: AntiCheatAnalyzer,
}

impl ModerationService {
    pub fn new(pool: PgPool, redis: redis::Client, publisher: EventPublisher) -> Self {
        let analyzer = AntiCheatAnalyzer::new(pool.clone());
        Self {
            pool,
            redis,
            publisher,
            analyzer,
        }
    }

    /// Check if user has an active chat mute sanction.
    pub async fn is_muted(&self, user_id: i64) -> Result<bool, ModerationError> {
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let cached_res: Result<Option<bool>, _> =
            conn.get(format!("sanction:mute:{user_id}")).await;
        if let Ok(Some(muted)) = cached_res {
            return Ok(muted);
        }

        let row = sqlx::query(
            r#"
            SELECT id FROM sanctions
            WHERE user_id = $1
              AND kind = 'mute'
              AND lifted_at IS NULL
              AND (expires_at IS NULL OR expires_at > now())
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        let is_muted = row.is_some();
        let _: Result<(), _> = conn
            .set_ex(format!("sanction:mute:{user_id}"), is_muted, 60)
            .await;

        Ok(is_muted)
    }

    /// In-game chat message submission.
    pub async fn send_chat(
        &self,
        user_id: i64,
        game_public_id: &str,
        raw_text: &str,
    ) -> Result<ChatMessageItem, ModerationError> {
        let text = raw_text.trim();
        if text.is_empty() || text.chars().count() > 200 {
            return Err(ModerationError::MessageTooLong);
        }

        // 1. Check mute
        if self.is_muted(user_id).await? {
            return Err(ModerationError::Muted);
        }

        // 2. Rate limit (1 message every 2 seconds per user per game)
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let rate_key = format!("chat:rate:{user_id}:{game_public_id}");
        let set_ok: bool = redis::cmd("SET")
            .arg(&rate_key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(2)
            .query_async(&mut conn)
            .await
            .unwrap_or(false);

        if !set_ok {
            return Err(ModerationError::RateLimited(
                "You can send at most 1 message every 2 seconds".into(),
            ));
        }

        // 3. Verify game exists and user is a participant
        let game_row =
            sqlx::query("SELECT id, white_user_id, black_user_id FROM games WHERE public_id = $1")
                .bind(game_public_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| ModerationError::GameNotFound(game_public_id.to_string()))?;

        let game_db_id: i64 = game_row.get("id");
        let white_id: i64 = game_row.get("white_user_id");
        let black_id: i64 = game_row.get("black_user_id");

        if user_id != white_id && user_id != black_id {
            return Err(ModerationError::NotInGame);
        }

        let opponent_id = if user_id == white_id {
            black_id
        } else {
            white_id
        };

        // 4. Verify no block between players
        let blocked = sqlx::query(
            "SELECT 1 FROM blocks WHERE (blocker_id = $1 AND blocked_id = $2) OR (blocker_id = $2 AND blocked_id = $1) LIMIT 1",
        )
        .bind(user_id)
        .bind(opponent_id)
        .fetch_optional(&self.pool)
        .await?;

        if blocked.is_some() {
            return Err(ModerationError::Blocked);
        }

        // 5. Mask profanity
        let (masked_text, was_masked) = mask_profanity(text);

        // 6. Insert into chat_messages
        let msg_row = sqlx::query(
            r#"
            INSERT INTO chat_messages (game_id, user_id, body, masked)
            VALUES ($1, $2, $3, $4)
            RETURNING id, created_at
            "#,
        )
        .bind(game_db_id)
        .bind(user_id)
        .bind(&masked_text)
        .bind(was_masked)
        .fetch_one(&self.pool)
        .await?;

        let msg_id: i64 = msg_row.get("id");
        let created_at: OffsetDateTime = msg_row.get("created_at");
        let created_at_iso = created_at.format(&Rfc3339).unwrap_or_default();

        // 7. Get sender public_id and handle
        let user_row = sqlx::query(
            r#"
            SELECT u.public_id, p.handle
            FROM users u
            JOIN profiles p ON p.user_id = u.id
            WHERE u.id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let public_id: String = user_row.get("public_id");
        let handle: String = user_row.get("handle");

        let item = ChatMessageItem {
            id: msg_id,
            game_id: game_public_id.to_string(),
            user_id: public_id,
            handle,
            body: masked_text,
            masked: was_masked,
            created_at: created_at_iso,
        };

        // 8. Publish chat.message to ws:g:{game_id}
        let _ = self
            .publisher
            .publish(&format!("g:{game_public_id}"), "chat.message", json!(item))
            .await;

        Ok(item)
    }

    /// Retrieve chat history for an active or completed game.
    pub async fn get_game_chat(
        &self,
        user_id: i64,
        game_public_id: &str,
        limit: i64,
    ) -> Result<Vec<ChatMessageItem>, ModerationError> {
        let game_row =
            sqlx::query("SELECT id, white_user_id, black_user_id FROM games WHERE public_id = $1")
                .bind(game_public_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| ModerationError::GameNotFound(game_public_id.to_string()))?;

        let game_db_id: i64 = game_row.get("id");
        let white_id: i64 = game_row.get("white_user_id");
        let black_id: i64 = game_row.get("black_user_id");

        if user_id != white_id && user_id != black_id {
            return Err(ModerationError::NotInGame);
        }

        let rows = sqlx::query(
            r#"
            SELECT
                m.id,
                u.public_id,
                p.handle,
                m.body,
                m.masked,
                m.created_at
            FROM chat_messages m
            JOIN users u ON u.id = m.user_id
            JOIN profiles p ON p.user_id = u.id
            WHERE m.game_id = $1 AND m.redacted = false
            ORDER BY m.created_at ASC
            LIMIT $2
            "#,
        )
        .bind(game_db_id)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let created_at: OffsetDateTime = r.get("created_at");
            out.push(ChatMessageItem {
                id: r.get("id"),
                game_id: game_public_id.to_string(),
                user_id: r.get("public_id"),
                handle: r.get("handle"),
                body: r.get("body"),
                masked: r.get("masked"),
                created_at: created_at.format(&Rfc3339).unwrap_or_default(),
            });
        }

        Ok(out)
    }

    /// Submit a safety/fair-play report against another user.
    pub async fn submit_report(
        &self,
        reporter_id: i64,
        req: CreateReportRequest,
    ) -> Result<ReportItem, ModerationError> {
        let category = req.category.to_lowercase();
        if !["cheating", "abuse", "inappropriate_name", "other"].contains(&category.as_str()) {
            return Err(ModerationError::InvalidCategory(category));
        }

        // Daily report rate limit: max 5 reports per 24 hours per user
        let mut conn = self.redis.get_multiplexed_async_connection().await?;
        let rate_key = format!("report:rate:{reporter_id}");
        let count: i64 = conn.incr(&rate_key, 1).await?;
        if count == 1 {
            let _: Result<(), _> = conn.expire(&rate_key, 86400).await;
        }
        if count > 5 {
            return Err(ModerationError::RateLimited(
                "Maximum 5 reports per day allowed".into(),
            ));
        }

        // Target user lookup
        let target_handle = req.target_handle.trim().trim_start_matches('@');
        let target_row = sqlx::query(
            "SELECT u.id, u.public_id FROM profiles p JOIN users u ON u.id = p.user_id WHERE p.handle = $1",
        )
        .bind(target_handle)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ModerationError::UserNotFound(target_handle.to_string()))?;

        let target_user_id: i64 = target_row.get("id");

        // Optional game DB id lookup
        let game_db_id: Option<i64> = if let Some(ref gid) = req.game_id {
            sqlx::query_scalar("SELECT id FROM games WHERE public_id = $1")
                .bind(gid)
                .fetch_optional(&self.pool)
                .await?
        } else {
            None
        };

        let note = req.note.as_deref().map(|n| n.trim());

        let row = sqlx::query(
            r#"
            INSERT INTO reports (reporter_id, category, target_user_id, game_id, note, status)
            VALUES ($1, $2, $3, $4, $5, 'open')
            RETURNING id, created_at, status
            "#,
        )
        .bind(reporter_id)
        .bind(&category)
        .bind(target_user_id)
        .bind(game_db_id)
        .bind(note)
        .fetch_one(&self.pool)
        .await?;

        let rep_row = sqlx::query("SELECT public_id FROM users WHERE id = $1")
            .bind(reporter_id)
            .fetch_one(&self.pool)
            .await?;
        let reporter_public_id: String = rep_row.get("public_id");

        let id: i64 = row.get("id");
        let created_at: OffsetDateTime = row.get("created_at");
        let status: String = row.get("status");

        Ok(ReportItem {
            id,
            reporter_id: reporter_public_id,
            category,
            target_handle: target_handle.to_string(),
            game_id: req.game_id,
            note: note.map(|s| s.to_string()),
            status,
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
        })
    }

    /// Return official IT Rules 2021 Grievance Officer details.
    pub fn get_grievance_info(&self) -> GrievanceInfo {
        GrievanceInfo {
            officer_name: "Priya Sharma".into(),
            officer_title: "Resident Grievance Officer (India)".into(),
            officer_email: "grievance-officer@chesshustle.com".into(),
            address: "Chess Hustle Interactive Pvt Ltd, Tech Park, Koramangala, Bengaluru, Karnataka 560095, India".into(),
            acknowledgment_sla_hours: 24,
            disposition_sla_days: 15,
            governing_rule: "Information Technology (Intermediary Guidelines and Digital Media Ethics Code) Rules, 2021".into(),
            policies: vec![
                GrievancePolicyLink {
                    name: "Terms of Service".into(),
                    url: "https://chesshustle.com/legal/tos".into(),
                },
                GrievancePolicyLink {
                    name: "Privacy & DPDP Policy".into(),
                    url: "https://chesshustle.com/legal/privacy".into(),
                },
                GrievancePolicyLink {
                    name: "Fair Play & Anti-Cheat Policy".into(),
                    url: "https://chesshustle.com/legal/fair-play".into(),
                },
            ],
        }
    }

    /// Run the offline anti-cheat analyzer on a finished game.
    pub async fn run_anti_cheat_analysis(
        &self,
        game_id: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        self.analyzer.analyze_game(game_id).await
    }

    /// List cheat detector anomaly flags with optional filtering by status and kind.
    pub async fn list_flags(
        &self,
        status: Option<&str>,
        kind: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT f.id, f.kind, f.subject_user_id, p_subj.handle AS subject_handle,
                   f.related_user_id, p_rel.handle AS related_handle,
                   f.score, f.evidence, f.status, f.reviewed_by, f.reviewed_at, f.created_at
            FROM anomaly_flags f
            LEFT JOIN profiles p_subj ON p_subj.user_id = f.subject_user_id
            LEFT JOIN profiles p_rel ON p_rel.user_id = f.related_user_id
            WHERE ($1::text IS NULL OR f.status = $1)
              AND ($2::text IS NULL OR f.kind = $2)
            ORDER BY f.created_at DESC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(status)
        .bind(kind)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let created_at: OffsetDateTime = r.get("created_at");
                let reviewed_at: Option<OffsetDateTime> = r.get("reviewed_at");
                AnomalyFlagItem {
                    id: r.get("id"),
                    kind: r.get("kind"),
                    subject_user_id: r.get("subject_user_id"),
                    subject_handle: r.get("subject_handle"),
                    related_user_id: r.get("related_user_id"),
                    related_handle: r.get("related_handle"),
                    score: r.get("score"),
                    evidence: r.get("evidence"),
                    status: r.get("status"),
                    reviewed_by: r.get("reviewed_by"),
                    reviewed_at: reviewed_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
                    created_at: created_at.format(&Rfc3339).unwrap_or_default(),
                }
            })
            .collect();

        Ok(items)
    }

    /// Resolve an anomaly flag (moderator decision: 'cleared' or 'confirmed').
    pub async fn resolve_flag(
        &self,
        flag_id: i64,
        actor: &str,
        req: &ResolveFlagRequest,
    ) -> Result<AnomalyFlagItem, ModerationError> {
        let res = req.resolution.to_lowercase();
        if res != "cleared" && res != "confirmed" && res != "reviewing" {
            return Err(ModerationError::InvalidStatus(format!(
                "Resolution must be 'cleared', 'confirmed', or 'reviewing', got '{}'",
                req.resolution
            )));
        }

        let row = sqlx::query(
            r#"
            UPDATE anomaly_flags
            SET status = $1, reviewed_by = $2, reviewed_at = now()
            WHERE id = $3
            RETURNING id, kind, subject_user_id, related_user_id, score, evidence, status, reviewed_by, reviewed_at, created_at
            "#,
        )
        .bind(&res)
        .bind(actor)
        .bind(flag_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ModerationError::NotFound(format!("Flag {flag_id} not found")))?;

        let created_at: OffsetDateTime = row.get("created_at");
        let reviewed_at: Option<OffsetDateTime> = row.get("reviewed_at");

        let _ = self
            .record_audit_log(
                actor,
                "resolve_flag",
                Some("anomaly_flag"),
                Some(&flag_id.to_string()),
                json!({
                    "resolution": res,
                    "notes": req.notes,
                }),
            )
            .await;

        Ok(AnomalyFlagItem {
            id: row.get("id"),
            kind: row.get("kind"),
            subject_user_id: row.get("subject_user_id"),
            subject_handle: None,
            related_user_id: row.get("related_user_id"),
            related_handle: None,
            score: row.get("score"),
            evidence: row.get("evidence"),
            status: row.get("status"),
            reviewed_by: row.get("reviewed_by"),
            reviewed_at: reviewed_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
        })
    }

    /// Apply a sanction on a user (warning, mute, mm_suspension, ad_suspension, suspension).
    pub async fn create_sanction(
        &self,
        user_id: i64,
        actor: &str,
        req: &CreateSanctionRequest,
    ) -> Result<SanctionItem, ModerationError> {
        let kind = req.kind.to_lowercase();
        const VALID_KINDS: &[&str] = &[
            "warning",
            "mute",
            "mm_suspension",
            "ad_suspension",
            "suspension",
        ];
        if !VALID_KINDS.contains(&kind.as_str()) {
            return Err(ModerationError::InvalidKind(format!(
                "Invalid sanction kind '{}'. Allowed: warning, mute, mm_suspension, ad_suspension, suspension",
                req.kind
            )));
        }

        let exp_dt = if let Some(ref exp_str) = req.expires_at {
            Some(OffsetDateTime::parse(exp_str, &Rfc3339).map_err(|_| {
                ModerationError::Validation(format!("Invalid expires_at RFC3339 format: {exp_str}"))
            })?)
        } else {
            None
        };

        let row = sqlx::query(
            r#"
            INSERT INTO sanctions (user_id, kind, reason, issued_by, evidence, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, user_id, kind, reason, issued_by, evidence, created_at, expires_at, lifted_at
            "#,
        )
        .bind(user_id)
        .bind(&kind)
        .bind(&req.reason)
        .bind(actor)
        .bind(req.evidence.clone())
        .bind(exp_dt)
        .fetch_one(&self.pool)
        .await?;

        // Apply immediate enforcement effects:
        if kind == "mute" {
            if let Ok(mut conn) = self.redis.get_multiplexed_async_connection().await {
                let ttl_secs = exp_dt
                    .map(|exp| (exp - OffsetDateTime::now_utc()).whole_seconds().max(60))
                    .unwrap_or(86400 * 30);
                let _: Result<(), _> = conn
                    .set_ex(format!("sanction:mute:{user_id}"), true, ttl_secs as u64)
                    .await;
            }
        } else if kind == "suspension" {
            let _ = sqlx::query("UPDATE users SET status = 'suspended' WHERE id = $1")
                .bind(user_id)
                .execute(&self.pool)
                .await;
        }

        let _ = self
            .record_audit_log(
                actor,
                "create_sanction",
                Some("user"),
                Some(&user_id.to_string()),
                json!({
                    "kind": kind,
                    "reason": req.reason,
                    "expires_at": req.expires_at,
                }),
            )
            .await;

        let created_at: OffsetDateTime = row.get("created_at");
        let expires_at: Option<OffsetDateTime> = row.get("expires_at");
        let lifted_at: Option<OffsetDateTime> = row.get("lifted_at");

        Ok(SanctionItem {
            id: row.get("id"),
            user_id: row.get("user_id"),
            handle: None,
            kind: row.get("kind"),
            reason: row.get("reason"),
            issued_by: row.get("issued_by"),
            evidence: row.get("evidence"),
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
            expires_at: expires_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
            lifted_at: lifted_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
        })
    }

    /// List sanctions applied to a user.
    pub async fn list_sanctions(&self, user_id: i64) -> Result<Vec<SanctionItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT s.id, s.user_id, p.handle, s.kind, s.reason, s.issued_by, s.evidence, s.created_at, s.expires_at, s.lifted_at
            FROM sanctions s
            LEFT JOIN profiles p ON p.user_id = s.user_id
            WHERE s.user_id = $1
            ORDER BY s.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let created_at: OffsetDateTime = r.get("created_at");
                let expires_at: Option<OffsetDateTime> = r.get("expires_at");
                let lifted_at: Option<OffsetDateTime> = r.get("lifted_at");
                SanctionItem {
                    id: r.get("id"),
                    user_id: r.get("user_id"),
                    handle: r.get("handle"),
                    kind: r.get("kind"),
                    reason: r.get("reason"),
                    issued_by: r.get("issued_by"),
                    evidence: r.get("evidence"),
                    created_at: created_at.format(&Rfc3339).unwrap_or_default(),
                    expires_at: expires_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
                    lifted_at: lifted_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
                }
            })
            .collect();

        Ok(items)
    }

    /// List user reports with optional status filtering.
    pub async fn list_reports(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ReportItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, u_rep.public_id AS reporter_id, r.category,
                   p_target.handle AS target_handle, g.public_id AS game_id,
                   r.note, r.status, r.created_at
            FROM reports r
            JOIN users u_rep ON u_rep.id = r.reporter_id
            JOIN profiles p_target ON p_target.user_id = r.target_user_id
            LEFT JOIN games g ON g.id = r.game_id
            WHERE ($1::text IS NULL OR r.status = $1)
            ORDER BY r.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let created_at: OffsetDateTime = r.get("created_at");
                ReportItem {
                    id: r.get("id"),
                    reporter_id: r.get("reporter_id"),
                    category: r.get("category"),
                    target_handle: r.get("target_handle"),
                    game_id: r.get("game_id"),
                    note: r.get("note"),
                    status: r.get("status"),
                    created_at: created_at.format(&Rfc3339).unwrap_or_default(),
                }
            })
            .collect();

        Ok(items)
    }

    /// Action or dismiss a user report.
    pub async fn action_report(
        &self,
        report_id: i64,
        actor: &str,
        req: &ActionReportRequest,
    ) -> Result<ReportItem, ModerationError> {
        let act = req.action.to_lowercase();
        if act != "actioned" && act != "dismissed" && act != "reviewing" {
            return Err(ModerationError::InvalidStatus(format!(
                "Action must be 'actioned', 'dismissed', or 'reviewing', got '{}'",
                req.action
            )));
        }

        let row = sqlx::query(
            r#"
            UPDATE reports
            SET status = $1, resolved_at = now(), resolution = $2
            WHERE id = $3
            RETURNING id, reporter_id, category, target_user_id, game_id, note, status, created_at
            "#,
        )
        .bind(&act)
        .bind(req.resolution.clone())
        .bind(report_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| ModerationError::NotFound(format!("Report {report_id} not found")))?;

        let _ = self
            .record_audit_log(
                actor,
                "action_report",
                Some("report"),
                Some(&report_id.to_string()),
                json!({
                    "action": act,
                    "resolution": req.resolution,
                }),
            )
            .await;

        let target_user_id: i64 = row.get("target_user_id");
        let target_handle: String =
            sqlx::query_scalar("SELECT handle FROM profiles WHERE user_id = $1")
                .bind(target_user_id)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_else(|| format!("player_{target_user_id}"));

        let reporter_id_i64: i64 = row.get("reporter_id");
        let reporter_public_id: String =
            sqlx::query_scalar("SELECT public_id FROM users WHERE id = $1")
                .bind(reporter_id_i64)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or_else(|| reporter_id_i64.to_string());

        let created_at: OffsetDateTime = row.get("created_at");

        Ok(ReportItem {
            id: row.get("id"),
            reporter_id: reporter_public_id,
            category: row.get("category"),
            target_handle,
            game_id: None,
            note: row.get("note"),
            status: row.get("status"),
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
        })
    }

    /// Retrieve detailed player information for admin inspection.
    pub async fn get_player_admin_detail(
        &self,
        identifier: &str,
    ) -> Result<AdminPlayerDetail, ModerationError> {
        let user_row = if let Ok(id) = identifier.parse::<i64>() {
            sqlx::query(
                r#"
                SELECT u.id, u.public_id, u.status, u.role, u.phone_e164, u.email, u.created_at,
                       p.handle, p.display_name
                FROM users u
                JOIN profiles p ON p.user_id = u.id
                WHERE u.id = $1
                "#,
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT u.id, u.public_id, u.status, u.role, u.phone_e164, u.email, u.created_at,
                       p.handle, p.display_name
                FROM users u
                JOIN profiles p ON p.user_id = u.id
                WHERE p.handle = $1 OR u.public_id = $1
                "#,
            )
            .bind(identifier)
            .fetch_optional(&self.pool)
            .await?
        };

        let u = user_row.ok_or_else(|| ModerationError::UserNotFound(identifier.to_string()))?;
        let uid: i64 = u.get("id");

        let gold_balance: i64 =
            sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
                .bind(uid)
                .fetch_optional(&self.pool)
                .await?
                .unwrap_or(0);

        let ratings_rows = sqlx::query("SELECT pool, rating FROM ratings WHERE user_id = $1")
            .bind(uid)
            .fetch_all(&self.pool)
            .await?;

        let mut ratings_map = serde_json::Map::new();
        for r in ratings_rows {
            let pool: String = r.get("pool");
            let rating: f32 = r.get("rating");
            ratings_map.insert(pool, json!(rating.round() as i64));
        }

        let sanctions = self.list_sanctions(uid).await?;

        let flags = self
            .list_flags(None, None, 50, 0)
            .await?
            .into_iter()
            .filter(|f| f.subject_user_id == uid || f.related_user_id == Some(uid))
            .collect();

        let reports_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reports WHERE target_user_id = $1")
                .bind(uid)
                .fetch_one(&self.pool)
                .await?;

        let created_at: OffsetDateTime = u.get("created_at");

        Ok(AdminPlayerDetail {
            id: uid,
            public_id: u.get("public_id"),
            handle: u.get("handle"),
            display_name: u.get("display_name"),
            role: u.get("role"),
            status: u.get("status"),
            phone_e164: u.get("phone_e164"),
            email: u.get("email"),
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
            gold_balance,
            ratings: serde_json::Value::Object(ratings_map),
            active_sanctions: sanctions,
            anomaly_flags: flags,
            reports_received: reports_count,
        })
    }

    /// Retrieve admin audit log entries.
    pub async fn get_audit_logs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AdminAuditItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, actor, action, target_type, target_id, payload, created_at
            FROM admin_audit_log
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let created_at: OffsetDateTime = r.get("created_at");
                AdminAuditItem {
                    id: r.get("id"),
                    actor: r.get("actor"),
                    action: r.get("action"),
                    target_type: r.get("target_type"),
                    target_id: r.get("target_id"),
                    payload: r.get("payload"),
                    created_at: created_at.format(&Rfc3339).unwrap_or_default(),
                }
            })
            .collect();

        Ok(items)
    }

    /// Record an entry in the admin audit log.
    pub async fn record_audit_log(
        &self,
        actor: &str,
        action: &str,
        target_type: Option<&str>,
        target_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<i64, ModerationError> {
        let row = sqlx::query(
            r#"
            INSERT INTO admin_audit_log (actor, action, target_type, target_id, payload)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(actor)
        .bind(action)
        .bind(target_type)
        .bind(target_id)
        .bind(payload)
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = row.get("id");
        Ok(id)
    }
}

fn mask_profanity(input: &str) -> (String, bool) {
    let mut modified = false;
    let mut out = input.to_string();

    for bad in PROFANITY_LIST {
        // Simple case-insensitive matching
        let lower = out.to_lowercase();
        if let Some(pos) = lower.find(bad) {
            modified = true;
            let stars = "*".repeat(bad.len());
            out.replace_range(pos..pos + bad.len(), &stars);
        }
    }

    (out, modified)
}
