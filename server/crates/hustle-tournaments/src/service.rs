use crate::domain::{
    CreateTournamentRequest, Tournament, TournamentDetail, TournamentEntry, TournamentError,
    TournamentPairing,
};
use crate::swiss::{compute_standings, generate_next_round_pairings};
use hustle_gold::{GoldReason, LedgerOp, PgGoldLedger};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct TournamentService {
    pool: PgPool,
}

impl TournamentService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List tournaments with optional status filtering.
    pub async fn list_tournaments(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Tournament>, TournamentError> {
        let tournaments = if let Some(st) = status {
            sqlx::query_as::<_, Tournament>(
                r#"
                SELECT id, public_id, name, tc, rounds, round_minutes, status,
                       starts_at, registration_closes_at, capacity, prize_config, created_by
                FROM tournaments
                WHERE status = $1
                ORDER BY starts_at ASC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(st)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Tournament>(
                r#"
                SELECT id, public_id, name, tc, rounds, round_minutes, status,
                       starts_at, registration_closes_at, capacity, prize_config, created_by
                FROM tournaments
                ORDER BY starts_at ASC
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(tournaments)
    }

    /// Retrieve detailed view of a tournament including current standings and round pairings.
    pub async fn get_tournament_detail(
        &self,
        public_id: &str,
        current_user_id: Option<i64>,
    ) -> Result<TournamentDetail, TournamentError> {
        let tournament = sqlx::query_as::<_, Tournament>(
            r#"
            SELECT id, public_id, name, tc, rounds, round_minutes, status,
                   starts_at, registration_closes_at, capacity, prize_config, created_by
            FROM tournaments
            WHERE public_id = $1
            "#,
        )
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| TournamentError::NotFound(public_id.to_string()))?;

        let entries_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tournament_entries WHERE tournament_id = $1 AND withdrawn = false",
        )
        .bind(tournament.id)
        .fetch_one(&self.pool)
        .await?;

        let my_entry = if let Some(uid) = current_user_id {
            sqlx::query_as::<_, TournamentEntry>(
                r#"
                SELECT tournament_id, user_id, joined_at, withdrawn, final_rank, prize_gold
                FROM tournament_entries
                WHERE tournament_id = $1 AND user_id = $2
                "#,
            )
            .bind(tournament.id)
            .bind(uid)
            .fetch_optional(&self.pool)
            .await?
        } else {
            None
        };

        let pairings = sqlx::query_as::<_, TournamentPairing>(
            r#"
            SELECT id, tournament_id, round, board, white_id, black_id, game_id, result
            FROM tournament_pairings
            WHERE tournament_id = $1
            ORDER BY round ASC, board ASC
            "#,
        )
        .bind(tournament.id)
        .fetch_all(&self.pool)
        .await?;

        // Fetch active player handles
        let player_rows = sqlx::query(
            r#"
            SELECT e.user_id, p.handle
            FROM tournament_entries e
            JOIN profiles p ON p.user_id = e.user_id
            WHERE e.tournament_id = $1 AND e.withdrawn = false
            "#,
        )
        .bind(tournament.id)
        .fetch_all(&self.pool)
        .await?;

        let mut handles = HashMap::new();
        let mut player_ids = Vec::new();
        for r in player_rows {
            let uid: i64 = r.get("user_id");
            let handle: String = r.get("handle");
            handles.insert(uid, handle);
            player_ids.push(uid);
        }

        let (_, standings) = compute_standings(&player_ids, &handles, &pairings);

        Ok(TournamentDetail {
            tournament,
            entries_count,
            my_entry,
            standings,
            pairings,
        })
    }

    /// Register for a tournament (free entry per ADR-011).
    pub async fn join_tournament(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<TournamentEntry, TournamentError> {
        let mut tx = self.pool.begin().await?;

        let tournament_row = sqlx::query(
            r#"
            SELECT id, status, registration_closes_at, capacity
            FROM tournaments
            WHERE public_id = $1
            FOR UPDATE
            "#,
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| TournamentError::NotFound(public_id.to_string()))?;

        let tournament_id: i64 = tournament_row.get("id");
        let status: String = tournament_row.get("status");
        let reg_closes: OffsetDateTime = tournament_row.get("registration_closes_at");
        let capacity: i32 = tournament_row.get("capacity");

        let now = OffsetDateTime::now_utc();
        if now > reg_closes {
            return Err(TournamentError::RegistrationClosed);
        }

        if status != "scheduled" && status != "registering" {
            return Err(TournamentError::InvalidState(format!(
                "Cannot register when tournament is '{status}'"
            )));
        }

        let active_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM tournament_entries WHERE tournament_id = $1 AND withdrawn = false",
        )
        .bind(tournament_id)
        .fetch_one(&mut *tx)
        .await?;

        if active_count >= capacity as i64 {
            return Err(TournamentError::Full);
        }

        let entry = sqlx::query_as::<_, TournamentEntry>(
            r#"
            INSERT INTO tournament_entries (tournament_id, user_id, joined_at, withdrawn)
            VALUES ($1, $2, now(), false)
            ON CONFLICT (tournament_id, user_id)
            DO UPDATE SET withdrawn = false, joined_at = now()
            RETURNING tournament_id, user_id, joined_at, withdrawn, final_rank, prize_gold
            "#,
        )
        .bind(tournament_id)
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(entry)
    }

    /// Withdraw from a tournament prior to registration closing.
    pub async fn withdraw_tournament(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<(), TournamentError> {
        let mut tx = self.pool.begin().await?;

        let tournament_row = sqlx::query(
            "SELECT id, status, registration_closes_at FROM tournaments WHERE public_id = $1",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| TournamentError::NotFound(public_id.to_string()))?;

        let tournament_id: i64 = tournament_row.get("id");
        let status: String = tournament_row.get("status");
        let reg_closes: OffsetDateTime = tournament_row.get("registration_closes_at");

        let now = OffsetDateTime::now_utc();
        if now > reg_closes || (status != "scheduled" && status != "registering") {
            return Err(TournamentError::InvalidState(
                "Withdrawal is only permitted while registration is open".into(),
            ));
        }

        let affected = sqlx::query(
            "UPDATE tournament_entries SET withdrawn = true WHERE tournament_id = $1 AND user_id = $2",
        )
        .bind(tournament_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(TournamentError::NotJoined);
        }

        tx.commit().await?;

        Ok(())
    }

    /// Create a tournament (admin role).
    pub async fn create_tournament(
        &self,
        created_by: i64,
        req: CreateTournamentRequest,
    ) -> Result<Tournament, TournamentError> {
        if req.rounds < 3 || req.rounds > 11 {
            return Err(TournamentError::Validation(
                "Rounds must be between 3 and 11".into(),
            ));
        }

        if req.capacity < 2 || req.capacity > 512 {
            return Err(TournamentError::Validation(
                "Capacity must be between 2 and 512".into(),
            ));
        }

        if req.registration_closes_at > req.starts_at {
            return Err(TournamentError::Validation(
                "Registration must close at or before start time".into(),
            ));
        }

        let public_id = format!("tn_{}", ulid::Ulid::new());
        let prize_config = req.prize_config.unwrap_or_else(|| {
            serde_json::json!({
                "splits": [
                    { "rank": 1, "gold": 10000 },
                    { "rank": 2, "gold": 5000 },
                    { "rank": 3, "gold": 2500 }
                ]
            })
        });

        let tournament = sqlx::query_as::<_, Tournament>(
            r#"
            INSERT INTO tournaments (
                public_id, name, tc, rounds, round_minutes, status,
                starts_at, registration_closes_at, capacity, prize_config, created_by
            )
            VALUES ($1, $2, $3, $4, $5, 'registering', $6, $7, $8, $9, $10)
            RETURNING id, public_id, name, tc, rounds, round_minutes, status,
                      starts_at, registration_closes_at, capacity, prize_config, created_by
            "#,
        )
        .bind(&public_id)
        .bind(&req.name)
        .bind(&req.tc)
        .bind(req.rounds)
        .bind(req.round_minutes)
        .bind(req.starts_at)
        .bind(req.registration_closes_at)
        .bind(req.capacity)
        .bind(&prize_config)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;

        Ok(tournament)
    }

    /// Advance tournament round: generates Swiss pairings for next round or finalizes standings & awards prizes.
    pub async fn advance_tournament(&self, tournament_id: i64) -> Result<Vec<TournamentPairing>, TournamentError> {
        let mut tx = self.pool.begin().await?;

        let tournament = sqlx::query_as::<_, Tournament>(
            r#"
            SELECT id, public_id, name, tc, rounds, round_minutes, status,
                   starts_at, registration_closes_at, capacity, prize_config, created_by
            FROM tournaments
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(tournament_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| TournamentError::NotFound(tournament_id.to_string()))?;

        let past_pairings = sqlx::query_as::<_, TournamentPairing>(
            r#"
            SELECT id, tournament_id, round, board, white_id, black_id, game_id, result
            FROM tournament_pairings
            WHERE tournament_id = $1
            ORDER BY round ASC, board ASC
            "#,
        )
        .bind(tournament_id)
        .fetch_all(&mut *tx)
        .await?;

        let current_round = past_pairings.iter().map(|p| p.round).max().unwrap_or(0);

        if current_round > 0 {
            // Verify all games in current round are concluded
            let pending_count = past_pairings
                .iter()
                .filter(|p| p.round == current_round && p.result.is_none())
                .count();

            if pending_count > 0 {
                return Err(TournamentError::InvalidRound(format!(
                    "Round {current_round} still has {pending_count} active games"
                )));
            }
        }

        // Fetch active players
        let player_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM tournament_entries WHERE tournament_id = $1 AND withdrawn = false",
        )
        .bind(tournament_id)
        .fetch_all(&mut *tx)
        .await?;

        if current_round < tournament.rounds {
            let next_round = current_round + 1;
            let next_pairings = generate_next_round_pairings(
                tournament_id,
                next_round,
                &player_ids,
                &past_pairings,
            );

            let mut created_pairings = Vec::new();
            for p in next_pairings {
                let row = sqlx::query_as::<_, TournamentPairing>(
                    r#"
                    INSERT INTO tournament_pairings (tournament_id, round, board, white_id, black_id, result)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    RETURNING id, tournament_id, round, board, white_id, black_id, game_id, result
                    "#,
                )
                .bind(tournament_id)
                .bind(p.round)
                .bind(p.board)
                .bind(p.white_id)
                .bind(p.black_id)
                .bind(p.result)
                .fetch_one(&mut *tx)
                .await?;
                created_pairings.push(row);
            }

            if tournament.status != "running" {
                sqlx::query("UPDATE tournaments SET status = 'running' WHERE id = $1")
                    .bind(tournament_id)
                    .execute(&mut *tx)
                    .await?;
            }

            tx.commit().await?;
            Ok(created_pairings)
        } else {
            // Finalize tournament: compute final standings and disburse Gold prizes
            let dummy_handles = HashMap::new();
            let (_, standings) = compute_standings(&player_ids, &dummy_handles, &past_pairings);

            let splits = tournament
                .prize_config
                .get("splits")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for item in &standings {
                let prize_gold = splits.iter().find_map(|s| {
                    let r = s.get("rank")?.as_i64()? as i32;
                    let gold = s.get("gold")?.as_i64()?;
                    if r == item.rank {
                        Some(gold)
                    } else {
                        None
                    }
                });

                if let Some(gold) = prize_gold {
                    let op = LedgerOp {
                        user_id: item.user_id,
                        delta: gold,
                        reason: GoldReason::Achievement,
                        ref_type: Some("tournament".to_string()),
                        ref_id: Some(tournament.public_id.clone()),
                        idem_key: Some(format!("tn:{}:{}:{}", tournament.id, item.rank, item.user_id)),
                        meta: serde_json::json!({
                            "tournament_id": tournament.id,
                            "rank": item.rank,
                            "points": item.points,
                        }),
                    };
                    PgGoldLedger::apply_in_tx(&mut tx, &op)
                        .await
                        .map_err(|e| TournamentError::Ledger(e.to_string()))?;
                }

                sqlx::query(
                    r#"
                    UPDATE tournament_entries
                    SET final_rank = $1, prize_gold = $2
                    WHERE tournament_id = $3 AND user_id = $4
                    "#,
                )
                .bind(item.rank)
                .bind(prize_gold)
                .bind(tournament_id)
                .bind(item.user_id)
                .execute(&mut *tx)
                .await?;
            }

            sqlx::query("UPDATE tournaments SET status = 'finished' WHERE id = $1")
                .bind(tournament_id)
                .execute(&mut *tx)
                .await?;

            tx.commit().await?;
            Ok(Vec::new())
        }
    }
}
