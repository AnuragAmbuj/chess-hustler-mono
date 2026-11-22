use crate::error::RewardError;
use crate::models::{AchievementItem, ClaimMissionResult, GameFinishedEvent, MissionItem};
use hustle_gold::{to_ist, GoldReason, LedgerOp, PgGoldLedger};
use sqlx::PgPool;
use time::OffsetDateTime;

pub fn daily_period_key(now: OffsetDateTime) -> String {
    let date = to_ist(now).date();
    format!(
        "{}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

pub fn weekly_period_key(now: OffsetDateTime) -> String {
    let date = to_ist(now).date();
    let year = date.year();
    let week = date.iso_week();
    format!("{year}-W{week:02}")
}

#[derive(Clone)]
pub struct RewardService {
    pool: PgPool,
}

impl RewardService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Ensure user mission rows exist for the active daily & weekly cycles,
    /// and return the populated list of active missions.
    pub async fn ensure_user_missions(
        &self,
        user_id: i64,
        now: OffsetDateTime,
    ) -> Result<Vec<MissionItem>, RewardError> {
        let daily_key = daily_period_key(now);
        let weekly_key = weekly_period_key(now);

        // Fetch all active mission definitions
        let defs: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT id, kind, goal_params
            FROM mission_defs
            WHERE active = true
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        for (id, kind, goal_params) in defs {
            let period_key = if kind == "daily" {
                &daily_key
            } else {
                &weekly_key
            };

            let target = goal_params
                .get("target")
                .and_then(|v| v.as_i64())
                .unwrap_or(1) as i32;

            sqlx::query(
                r#"
                INSERT INTO user_missions (user_id, mission_id, period_key, progress, goal)
                VALUES ($1, $2, $3, 0, $4)
                ON CONFLICT (user_id, mission_id, period_key) DO NOTHING
                "#,
            )
            .bind(user_id)
            .bind(&id)
            .bind(period_key)
            .bind(target)
            .execute(&self.pool)
            .await?;
        }

        // Return current active missions with progress
        let missions = sqlx::query_as::<_, MissionItem>(
            r#"
            SELECT
                md.id,
                md.kind,
                md.title_key,
                md.goal_type,
                um.progress,
                um.goal,
                um.claimed,
                md.reward_gold,
                um.period_key
            FROM user_missions um
            JOIN mission_defs md ON md.id = um.mission_id
            WHERE um.user_id = $1
              AND ((md.kind = 'daily' AND um.period_key = $2) OR (md.kind = 'weekly' AND um.period_key = $3))
            ORDER BY md.kind ASC, md.reward_gold ASC, md.id ASC
            "#,
        )
        .bind(user_id)
        .bind(&daily_key)
        .bind(&weekly_key)
        .fetch_all(&self.pool)
        .await?;

        Ok(missions)
    }

    /// Retrieve active missions with progress for user.
    pub async fn get_user_missions(
        &self,
        user_id: i64,
        now: OffsetDateTime,
    ) -> Result<Vec<MissionItem>, RewardError> {
        self.ensure_user_missions(user_id, now).await
    }

    /// Claim completed mission reward. Idempotent per user, mission, and period.
    pub async fn claim_mission(
        &self,
        user_id: i64,
        mission_id: &str,
        now: OffsetDateTime,
    ) -> Result<ClaimMissionResult, RewardError> {
        let def: Option<(String, i64)> = sqlx::query_as(
            "SELECT kind, reward_gold FROM mission_defs WHERE id = $1 AND active = true",
        )
        .bind(mission_id)
        .fetch_optional(&self.pool)
        .await?;

        let (kind, reward_gold) = match def {
            Some(row) => row,
            None => return Err(RewardError::MissionNotFound(mission_id.to_string())),
        };

        let period_key = if kind == "daily" {
            daily_period_key(now)
        } else {
            weekly_period_key(now)
        };

        let mut tx = self.pool.begin().await?;

        // 1. Lock and inspect user mission row
        let mission: Option<(i32, i32, bool)> = sqlx::query_as(
            r#"
            SELECT progress, goal, claimed
            FROM user_missions
            WHERE user_id = $1 AND mission_id = $2 AND period_key = $3
            FOR UPDATE
            "#,
        )
        .bind(user_id)
        .bind(mission_id)
        .bind(&period_key)
        .fetch_optional(&mut *tx)
        .await?;

        let (progress, goal, claimed) = match mission {
            Some(row) => row,
            None => return Err(RewardError::MissionNotFound(mission_id.to_string())),
        };

        if claimed {
            return Err(RewardError::AlreadyClaimed(mission_id.to_string()));
        }

        if progress < goal {
            return Err(RewardError::NotCompleted {
                current: progress,
                goal,
            });
        }

        // 2. Mark claimed
        sqlx::query(
            r#"
            UPDATE user_missions
            SET claimed = true, claimed_at = now()
            WHERE user_id = $1 AND mission_id = $2 AND period_key = $3
            "#,
        )
        .bind(user_id)
        .bind(mission_id)
        .bind(&period_key)
        .execute(&mut *tx)
        .await?;

        // 3. Grant Gold in ledger atomically
        let idem_key = format!("mission:{user_id}:{mission_id}:{period_key}");
        let op = LedgerOp {
            user_id,
            delta: reward_gold,
            reason: GoldReason::Mission,
            ref_type: Some("mission".to_string()),
            ref_id: Some(mission_id.to_string()),
            idem_key: Some(idem_key),
            meta: serde_json::json!({
                "mission_id": mission_id,
                "period_key": period_key,
            }),
        };

        let grant = PgGoldLedger::apply_in_tx(&mut tx, &op).await?;
        let balance = match grant {
            hustle_gold::GrantResult::Applied { balance_after, .. } => balance_after,
            hustle_gold::GrantResult::Duplicate { balance_after } => balance_after,
        };

        tx.commit().await?;

        Ok(ClaimMissionResult {
            claimed: true,
            amount: reward_gold,
            new_balance: balance,
        })
    }

    /// Retrieve achievements list with user progress and unlocked status.
    pub async fn get_user_achievements(
        &self,
        user_id: i64,
    ) -> Result<Vec<AchievementItem>, RewardError> {
        // Query user profile counters
        let profile_stats: Option<(i32, i32)> =
            sqlx::query_as("SELECT games_count, wins FROM profiles WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;

        let (games_count, wins) = profile_stats.unwrap_or((0, 0));

        // Query user peak rating across pools
        let peak_rating: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(rating), 1200)
            FROM ratings
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(1200);

        // Query max streak
        let streak_days: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(streak), 0)
            FROM daily_claims
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        // Query checkmates delivered
        let checkmate_count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM games
            WHERE status = 'finished'
              AND termination = 'checkmate'
              AND ((result = 'white' AND white_user_id = $1) OR (result = 'black' AND black_user_id = $1))
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        // Query all active achievements
        let defs: Vec<(String, String, String, serde_json::Value, i64)> = sqlx::query_as(
            r#"
            SELECT id, title_key, goal_type, goal_params, reward_gold
            FROM achievement_defs
            WHERE active = true
            ORDER BY reward_gold ASC, id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        // Query unlocked achievements
        let unlocked_rows: Vec<(String, OffsetDateTime)> = sqlx::query_as(
            r#"
            SELECT achievement_id, granted_at
            FROM user_achievements
            WHERE user_id = $1
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let unlocked_map: std::collections::HashMap<String, OffsetDateTime> =
            unlocked_rows.into_iter().collect();

        let items = defs
            .into_iter()
            .map(|(id, title_key, goal_type, goal_params, reward_gold)| {
                let target = goal_params
                    .get("target")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);

                let is_unlocked = unlocked_map.contains_key(&id);
                let granted_at = unlocked_map.get(&id).copied();

                let progress = if is_unlocked {
                    target
                } else {
                    let stat = match goal_type.as_str() {
                        "games_played" => games_count as i64,
                        "wins" => wins as i64,
                        "checkmates" => checkmate_count,
                        "streak_days" => streak_days,
                        "rating_peak" => peak_rating,
                        _ => 0,
                    };
                    stat.min(target)
                };

                AchievementItem {
                    id,
                    title_key,
                    goal_type,
                    progress,
                    target,
                    reward_gold,
                    unlocked: is_unlocked,
                    granted_at,
                }
            })
            .collect();

        Ok(items)
    }

    /// Server-authoritative event interception when a game finishes.
    /// Increments user mission progress and checks achievement criteria.
    pub async fn record_game_finished(
        &self,
        event: &GameFinishedEvent,
        now: OffsetDateTime,
    ) -> Result<(), RewardError> {
        let daily_key = daily_period_key(now);
        let weekly_key = weekly_period_key(now);

        let players = [event.white_user_id, event.black_user_id];

        // 1. Ensure missions are seeded for both players
        for &uid in &players {
            let _ = self.ensure_user_missions(uid, now).await;
        }

        // 2. Increment games_played missions for both players
        for &uid in &players {
            sqlx::query(
                r#"
                UPDATE user_missions
                SET progress = progress + 1
                WHERE user_id = $1
                  AND mission_id IN (SELECT id FROM mission_defs WHERE goal_type = 'games_played')
                  AND (period_key = $2 OR period_key = $3)
                  AND NOT claimed
                "#,
            )
            .bind(uid)
            .bind(&daily_key)
            .bind(&weekly_key)
            .execute(&self.pool)
            .await?;
        }

        // 3. Increment wins and checkmates for winner
        if let Some(winner) = event.winner_user_id {
            sqlx::query(
                r#"
                UPDATE user_missions
                SET progress = progress + 1
                WHERE user_id = $1
                  AND mission_id IN (SELECT id FROM mission_defs WHERE goal_type = 'wins')
                  AND (period_key = $2 OR period_key = $3)
                  AND NOT claimed
                "#,
            )
            .bind(winner)
            .bind(&daily_key)
            .bind(&weekly_key)
            .execute(&self.pool)
            .await?;

            if event.checkmate {
                sqlx::query(
                    r#"
                    UPDATE user_missions
                    SET progress = progress + 1
                    WHERE user_id = $1
                      AND mission_id IN (SELECT id FROM mission_defs WHERE goal_type = 'checkmates')
                      AND (period_key = $2 OR period_key = $3)
                      AND NOT claimed
                    "#,
                )
                .bind(winner)
                .bind(&daily_key)
                .bind(&weekly_key)
                .execute(&self.pool)
                .await?;
            }
        }

        // 4. Increment draws missions if drawn
        if event.draw {
            for &uid in &players {
                sqlx::query(
                    r#"
                    UPDATE user_missions
                    SET progress = progress + 1
                    WHERE user_id = $1
                      AND mission_id IN (SELECT id FROM mission_defs WHERE goal_type = 'draws')
                      AND (period_key = $2 OR period_key = $3)
                      AND NOT claimed
                    "#,
                )
                .bind(uid)
                .bind(&daily_key)
                .bind(&weekly_key)
                .execute(&self.pool)
                .await?;
            }
        }

        // 5. Evaluate achievements for both players
        for &uid in &players {
            self.evaluate_achievements(uid).await?;
        }

        Ok(())
    }

    /// Evaluate milestone achievements for a player and award any unlocked rewards.
    pub async fn evaluate_achievements(&self, user_id: i64) -> Result<Vec<String>, RewardError> {
        let achievements = self.get_user_achievements(user_id).await?;
        let mut unlocked = Vec::new();

        for ach in achievements {
            if !ach.unlocked && ach.progress >= ach.target {
                let mut tx = self.pool.begin().await?;

                let inserted: Option<String> = sqlx::query_scalar(
                    r#"
                    INSERT INTO user_achievements (user_id, achievement_id, granted_at)
                    VALUES ($1, $2, now())
                    ON CONFLICT (user_id, achievement_id) DO NOTHING
                    RETURNING achievement_id
                    "#,
                )
                .bind(user_id)
                .bind(&ach.id)
                .fetch_optional(&mut *tx)
                .await?;

                if inserted.is_some() {
                    let idem_key = format!("achievement:{user_id}:{}", ach.id);
                    let op = LedgerOp {
                        user_id,
                        delta: ach.reward_gold,
                        reason: GoldReason::Achievement,
                        ref_type: Some("achievement".to_string()),
                        ref_id: Some(ach.id.clone()),
                        idem_key: Some(idem_key),
                        meta: serde_json::json!({ "achievement_id": ach.id }),
                    };

                    PgGoldLedger::apply_in_tx(&mut tx, &op).await?;
                    tx.commit().await?;

                    tracing::info!(user_id, achievement_id = %ach.id, reward = ach.reward_gold, "achievement unlocked");
                    unlocked.push(ach.id);
                }
            }
        }

        Ok(unlocked)
    }
}
