//! Database service layer for games and moves (ADR-001).
//!
//! Manages game lifecycle rows in Postgres: creation, retrieval, moves application, and completion.

use crate::clock::{ClockError, ClockState, Side};
use crate::rules::{fen_key, RulesEngine, STARTING_FEN};
use hustle_domain::{Pool, TimeControl};
use hustle_gold::{GoldReason, GrantResult, LedgerError, LedgerOp, PgGoldLedger};
use hustle_ratings::{Outcome, PgRatingApplier, RatingApplier, RatingsError};
use serde::{Deserialize, Serialize};
use shakmaty::Color;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use ulid::Ulid;

#[derive(Debug, Error)]
pub enum GameServiceError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("players must be distinct")]
    SelfPlayNotAllowed,
    #[error("game not found: {0}")]
    NotFound(String),
    #[error("user is not a participant in this game")]
    NotParticipant,
    #[error("not user's turn to move")]
    WrongTurn,
    #[error("illegal move: {0}")]
    IllegalMove(String),
    #[error("game is already finished with status: {0}")]
    GameFinished(String),
    #[error("move rate limited (minimum 40ms interval)")]
    RateLimited,
    #[error("player flagged on time")]
    Flagged,
    #[error("cannot abort game: {0}")]
    CannotAbort(String),
    #[error("no active draw offer to accept or decline")]
    NoDrawOffer,
    #[error("cannot accept or decline your own draw offer")]
    OwnDrawOffer,
    #[error("clock has not expired yet")]
    ClockNotExpired,
    #[error("cannot claim timeout on your own turn")]
    NotOpponentTurn,
    #[error("ratings error: {0}")]
    Ratings(#[from] RatingsError),
    #[error("ledger error: {0}")]
    Ledger(#[from] LedgerError),
    #[error("invalid game state: {0}")]
    InvalidState(String),
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameRow {
    pub id: i64,
    pub public_id: String,
    pub white_user_id: i64,
    pub black_user_id: i64,
    pub pool: String,
    pub time_control: serde_json::Value,
    pub rated: bool,
    pub status: String,
    pub result: Option<String>,
    pub termination: Option<String>,
    pub fen_start: String,
    pub fen_current: String,
    pub move_count: i32,
    pub w_remaining_ms: Option<i64>,
    pub b_remaining_ms: Option<i64>,
    pub last_tick_ts: Option<time::OffsetDateTime>,
    pub version: i32,
    pub created_at: time::OffsetDateTime,
    pub started_at: Option<time::OffsetDateTime>,
    pub ended_at: Option<time::OffsetDateTime>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MoveRow {
    pub game_id: i64,
    pub ply: i32,
    pub uci: String,
    pub san: String,
    pub fen_after: String,
    pub w_remaining_ms: i64,
    pub b_remaining_ms: i64,
    pub ts: time::OffsetDateTime,
    pub meta: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MoveClocks {
    pub white_ms: i64,
    pub black_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResultData {
    pub ply: i32,
    pub uci: String,
    pub san: String,
    pub fen: String,
    pub clocks: MoveClocks,
    pub ts: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingDeltaDto {
    pub before: f64,
    pub after: f64,
    pub delta: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRatings {
    pub white: RatingDeltaDto,
    pub black: RatingDeltaDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameGoldEarnings {
    pub white_earned: i64,
    pub black_earned: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameCompletionData {
    pub result: String,
    pub termination: String,
    pub ratings: Option<GameRatings>,
    pub gold: GameGoldEarnings,
}

#[derive(Debug, Clone)]
pub struct ApplyMoveSuccess {
    pub move_data: MoveResultData,
    pub completion: Option<GameCompletionData>,
    pub game: GameRow,
}

#[derive(Debug, Clone)]
pub struct ResignSuccess {
    pub game: GameRow,
    pub result: String,
    pub termination: String,
    pub ratings: Option<GameRatings>,
    pub user_gold_earned: i64,
    pub user_gold_balance: i64,
}

#[derive(Clone)]
pub struct GameService {
    pool: PgPool,
}

impl GameService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Create a new game in `status = 'starting'` between two players.
    pub async fn create_game(
        &self,
        white_user_id: i64,
        black_user_id: i64,
        pool: Pool,
        tc: TimeControl,
        rated: bool,
    ) -> Result<GameRow, GameServiceError> {
        if white_user_id == black_user_id {
            return Err(GameServiceError::SelfPlayNotAllowed);
        }

        let public_id = Ulid::new().to_string();
        let tc_json = serde_json::json!({
            "initial_ms": tc.initial_ms,
            "increment_ms": tc.increment_ms,
        });

        let row = sqlx::query_as::<_, GameRow>(
            "INSERT INTO games (
                public_id, white_user_id, black_user_id, pool, time_control,
                rated, status, fen_start, fen_current, move_count,
                w_remaining_ms, b_remaining_ms, last_tick_ts, version
             )
             VALUES ($1, $2, $3, $4, $5, $6, 'starting', $7, $7, 0, $8, $8, now(), 0)
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(&public_id)
        .bind(white_user_id)
        .bind(black_user_id)
        .bind(pool.as_str())
        .bind(tc_json)
        .bind(rated)
        .bind(STARTING_FEN)
        .bind(tc.initial_ms as i64)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find game by public ULID.
    pub async fn get_game_by_public_id(
        &self,
        public_id: &str,
    ) -> Result<Option<GameRow>, GameServiceError> {
        let row = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1",
        )
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Find game by internal id.
    pub async fn get_game_by_id(&self, id: i64) -> Result<Option<GameRow>, GameServiceError> {
        let row = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
    }

    /// Mark game as active when both players are connected/ready.
    pub async fn start_game(&self, public_id: &str) -> Result<GameRow, GameServiceError> {
        let row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET status = 'active', started_at = coalesce(started_at, now()), last_tick_ts = now()
             WHERE public_id = $1 AND status = 'starting'
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(public_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        Ok(row)
    }

    /// Fetch all plies played for a game ordered by ply ascending.
    pub async fn get_moves_by_game_id(
        &self,
        game_id: i64,
    ) -> Result<Vec<MoveRow>, GameServiceError> {
        let rows = sqlx::query_as::<_, MoveRow>(
            "SELECT game_id, ply, uci, san, fen_after, w_remaining_ms, b_remaining_ms, ts, meta
             FROM moves
             WHERE game_id = $1
             ORDER BY ply ASC",
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Fetch all plies played for a game by public ULID.
    pub async fn get_moves_by_public_id(
        &self,
        public_id: &str,
    ) -> Result<Vec<MoveRow>, GameServiceError> {
        let rows = sqlx::query_as::<_, MoveRow>(
            "SELECT m.game_id, m.ply, m.uci, m.san, m.fen_after, m.w_remaining_ms, m.b_remaining_ms, m.ts, m.meta
             FROM moves m
             JOIN games g ON g.id = m.game_id
             WHERE g.public_id = $1
             ORDER BY m.ply ASC",
        )
        .bind(public_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// Execute a move in an active game at the current server time.
    pub async fn apply_move(
        &self,
        public_id: &str,
        user_id: i64,
        uci_move: &str,
    ) -> Result<ApplyMoveSuccess, GameServiceError> {
        self.apply_move_at(
            public_id,
            user_id,
            uci_move,
            time::OffsetDateTime::now_utc(),
        )
        .await
    }

    /// Execute a move in an active game at a specific timestamp (server-authoritative).
    pub async fn apply_move_at(
        &self,
        public_id: &str,
        user_id: i64,
        uci_move: &str,
        now: time::OffsetDateTime,
    ) -> Result<ApplyMoveSuccess, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        if game.status != "active" && game.status != "starting" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        // Fetch prior move FENs to reconstruct repetition keys
        let prior_fens: Vec<String> =
            sqlx::query_scalar("SELECT fen_after FROM moves WHERE game_id = $1 ORDER BY ply ASC")
                .bind(game.id)
                .fetch_all(&mut *tx)
                .await?;

        let mut prior_keys = Vec::with_capacity(prior_fens.len() + 1);
        if !prior_fens.is_empty() {
            prior_keys.push(fen_key(&game.fen_start));
            for fen in &prior_fens[..prior_fens.len().saturating_sub(1)] {
                prior_keys.push(fen_key(fen));
            }
        }

        let mut engine = RulesEngine::from_fen_with_keys(&game.fen_current, prior_keys)
            .map_err(|e| GameServiceError::IllegalMove(e.to_string()))?;

        let to_move = engine.to_move();
        if to_move == Color::White && !is_white {
            return Err(GameServiceError::WrongTurn);
        }
        if to_move == Color::Black && !is_black {
            return Err(GameServiceError::WrongTurn);
        }

        let now_ms = (now.unix_timestamp_nanos() / 1_000_000) as i64;
        let last_tick_ms = game
            .last_tick_ts
            .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
            .unwrap_or(now_ms);

        // Anti-spam rate limit: moves >= 40ms apart
        if game.move_count > 0 && !ClockState::rate_ok(last_tick_ms, now_ms) {
            return Err(GameServiceError::RateLimited);
        }

        let tc_initial_ms = game
            .time_control
            .get("initial_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(180_000);
        let tc_increment_ms = game
            .time_control
            .get("increment_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        let clock = ClockState {
            white_ms: game.w_remaining_ms.unwrap_or(tc_initial_ms),
            black_ms: game.b_remaining_ms.unwrap_or(tc_initial_ms),
            last_tick_ms,
        };

        let mover_side = if is_white { Side::White } else { Side::Black };
        let new_clock = match clock.advance(mover_side, now_ms, tc_increment_ms) {
            Ok(c) => c,
            Err(ClockError::Flagged) => {
                let opponent_side = mover_side.opponent();
                let opponent_color = match opponent_side {
                    Side::White => Color::White,
                    Side::Black => Color::Black,
                };
                let mover_color = match mover_side {
                    Side::White => Color::White,
                    Side::Black => Color::Black,
                };
                let opponent_can_mate =
                    RulesEngine::can_potentially_mate(engine.board(), opponent_color, mover_color);
                let (result, termination) = if opponent_can_mate {
                    (opponent_side.as_str(), "timeout")
                } else {
                    ("draw", "timeout")
                };

                let (game_row, completion, _) = Self::finalize_game_in_tx(
                    &mut tx,
                    &game,
                    FinalizeParams {
                        result,
                        termination,
                        ended_at: now,
                        final_fen: &game.fen_current,
                        move_count: game.move_count,
                        version: game.version + 1,
                        w_remaining_ms: game.w_remaining_ms.unwrap_or(tc_initial_ms),
                        b_remaining_ms: game.b_remaining_ms.unwrap_or(tc_initial_ms),
                        last_tick_ts: now,
                    },
                )
                .await?;

                tx.commit().await?;

                let now_iso = now
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default();
                return Ok(ApplyMoveSuccess {
                    move_data: MoveResultData {
                        ply: game.move_count,
                        uci: uci_move.to_string(),
                        san: "".to_string(),
                        fen: game.fen_current.clone(),
                        clocks: MoveClocks {
                            white_ms: game.w_remaining_ms.unwrap_or(tc_initial_ms),
                            black_ms: game.b_remaining_ms.unwrap_or(tc_initial_ms),
                        },
                        ts: now_iso,
                        version: game.version + 1,
                    },
                    completion: Some(completion),
                    game: game_row,
                });
            }
            Err(ClockError::BadClock) => {
                return Err(GameServiceError::InvalidState(
                    "clock monotonicity error".into(),
                ));
            }
        };

        let (played_move, auto_outcome) = engine
            .play_uci(uci_move)
            .map_err(|e| GameServiceError::IllegalMove(e.to_string()))?;

        let next_ply = game.move_count + 1;
        let next_version = game.version + 1;

        sqlx::query(
            "INSERT INTO moves (game_id, ply, uci, san, fen_after, w_remaining_ms, b_remaining_ms, ts, meta)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(game.id)
        .bind(next_ply)
        .bind(&played_move.uci)
        .bind(&played_move.san)
        .bind(&played_move.fen_after)
        .bind(new_clock.white_ms)
        .bind(new_clock.black_ms)
        .bind(now)
        .bind(serde_json::json!({}))
        .execute(&mut *tx)
        .await?;

        let now_iso = now
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let move_data = MoveResultData {
            ply: next_ply,
            uci: played_move.uci.clone(),
            san: played_move.san.clone(),
            fen: played_move.fen_after.clone(),
            clocks: MoveClocks {
                white_ms: new_clock.white_ms,
                black_ms: new_clock.black_ms,
            },
            ts: now_iso,
            version: next_version,
        };

        let (updated_game, completion) = match auto_outcome.terminal() {
            Some((result, termination)) => {
                let (game_row, completion, _) = Self::finalize_game_in_tx(
                    &mut tx,
                    &game,
                    FinalizeParams {
                        result,
                        termination: termination.as_str(),
                        ended_at: now,
                        final_fen: &played_move.fen_after,
                        move_count: next_ply,
                        version: next_version,
                        w_remaining_ms: new_clock.white_ms,
                        b_remaining_ms: new_clock.black_ms,
                        last_tick_ts: now,
                    },
                )
                .await?;
                (game_row, Some(completion))
            }
            None => {
                let row = sqlx::query_as::<_, GameRow>(
                    "UPDATE games
                     SET fen_current = $2, move_count = $3, version = $4,
                         w_remaining_ms = $5, b_remaining_ms = $6, last_tick_ts = $7,
                         status = 'active', started_at = coalesce(started_at, $7)
                     WHERE id = $1
                     RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                               rated, status, result, termination, fen_start, fen_current,
                               move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                               version, created_at, started_at, ended_at, meta",
                )
                .bind(game.id)
                .bind(&played_move.fen_after)
                .bind(next_ply)
                .bind(next_version)
                .bind(new_clock.white_ms)
                .bind(new_clock.black_ms)
                .bind(now)
                .fetch_one(&mut *tx)
                .await?;
                (row, None)
            }
        };

        tx.commit().await?;

        Ok(ApplyMoveSuccess {
            move_data,
            completion,
            game: updated_game,
        })
    }

    /// Resign an active game. Instant and idempotent.
    pub async fn resign_game(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<ResignSuccess, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        // Idempotency: if already finished, return existing state
        if game.status == "finished" {
            let user_balance: i64 =
                sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
                    .bind(user_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or(0);
            let earned = match game.result.as_deref() {
                Some("white") if is_white => 25,
                Some("black") if is_black => 25,
                Some("draw") => 10,
                _ => 5,
            };
            tx.rollback().await?;
            return Ok(ResignSuccess {
                result: game.result.clone().unwrap_or_default(),
                termination: game.termination.clone().unwrap_or_default(),
                ratings: None,
                user_gold_earned: earned,
                user_gold_balance: user_balance,
                game,
            });
        }

        if game.status != "active" && game.status != "starting" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let result = if is_white { "black" } else { "white" };
        let termination = "resignation";
        let now = time::OffsetDateTime::now_utc();
        let tc_initial_ms = game
            .time_control
            .get("initial_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(180_000);

        let (row, completion, (white_balance, black_balance)) = Self::finalize_game_in_tx(
            &mut tx,
            &game,
            FinalizeParams {
                result,
                termination,
                ended_at: now,
                final_fen: &game.fen_current,
                move_count: game.move_count,
                version: game.version + 1,
                w_remaining_ms: game.w_remaining_ms.unwrap_or(tc_initial_ms),
                b_remaining_ms: game.b_remaining_ms.unwrap_or(tc_initial_ms),
                last_tick_ts: now,
            },
        )
        .await?;

        tx.commit().await?;

        let (user_earned, user_balance) = if is_white {
            (completion.gold.white_earned, white_balance)
        } else {
            (completion.gold.black_earned, black_balance)
        };

        Ok(ResignSuccess {
            game: row,
            result: result.to_string(),
            termination: termination.to_string(),
            ratings: completion.ratings,
            user_gold_earned: user_earned,
            user_gold_balance: user_balance,
        })
    }

    /// Abort a game (valid only if move_count <= 4). No rating deltas, no Gold awarded.
    pub async fn abort_game(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<GameRow, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        if game.status == "aborted" {
            tx.rollback().await?;
            return Ok(game);
        }

        if game.status != "starting" && game.status != "active" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        if game.move_count > 4 {
            return Err(GameServiceError::CannotAbort(
                "game cannot be aborted after 4 plies".to_string(),
            ));
        }

        let row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET status = 'aborted', termination = 'aborted', ended_at = now(), version = version + 1
             WHERE id = $1
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(game.id)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    /// Offer a draw to opponent.
    pub async fn offer_draw(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<GameRow, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        if game.status != "active" && game.status != "starting" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let side_str = if is_white { "white" } else { "black" };
        let mut meta = game.meta.clone();
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "draw_offer".to_string(),
                serde_json::Value::String(side_str.to_string()),
            );
        } else {
            meta = serde_json::json!({ "draw_offer": side_str });
        }

        let row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET meta = $2, version = version + 1
             WHERE id = $1
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(game.id)
        .bind(meta)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    /// Decline opponent's draw offer.
    pub async fn decline_draw(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<GameRow, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        if game.status != "active" && game.status != "starting" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let current_offer = game.meta.get("draw_offer").and_then(|v| v.as_str());
        match current_offer {
            None => return Err(GameServiceError::NoDrawOffer),
            Some("white") if is_white => return Err(GameServiceError::OwnDrawOffer),
            Some("black") if is_black => return Err(GameServiceError::OwnDrawOffer),
            _ => {}
        }

        let mut meta = game.meta.clone();
        if let Some(obj) = meta.as_object_mut() {
            obj.remove("draw_offer");
        }

        let row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET meta = $2, version = version + 1
             WHERE id = $1
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(game.id)
        .bind(meta)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    /// Accept opponent's draw offer, ending game by mutual agreement.
    pub async fn accept_draw(
        &self,
        public_id: &str,
        user_id: i64,
    ) -> Result<ResignSuccess, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = user_id == game.white_user_id;
        let is_black = user_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        if game.status == "finished" {
            let user_balance: i64 =
                sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
                    .bind(user_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or(0);
            tx.rollback().await?;
            return Ok(ResignSuccess {
                game,
                result: "draw".to_string(),
                termination: "agreement".to_string(),
                ratings: None,
                user_gold_earned: 10,
                user_gold_balance: user_balance,
            });
        }

        if game.status != "active" && game.status != "starting" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let current_offer = game.meta.get("draw_offer").and_then(|v| v.as_str());
        match current_offer {
            None => return Err(GameServiceError::NoDrawOffer),
            Some("white") if is_white => return Err(GameServiceError::OwnDrawOffer),
            Some("black") if is_black => return Err(GameServiceError::OwnDrawOffer),
            _ => {}
        }

        let now = time::OffsetDateTime::now_utc();
        let tc_initial_ms = game
            .time_control
            .get("initial_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(180_000);

        let (row, completion, (white_balance, black_balance)) = Self::finalize_game_in_tx(
            &mut tx,
            &game,
            FinalizeParams {
                result: "draw",
                termination: "agreement",
                ended_at: now,
                final_fen: &game.fen_current,
                move_count: game.move_count,
                version: game.version + 1,
                w_remaining_ms: game.w_remaining_ms.unwrap_or(tc_initial_ms),
                b_remaining_ms: game.b_remaining_ms.unwrap_or(tc_initial_ms),
                last_tick_ts: now,
            },
        )
        .await?;

        let mut meta = row.meta.clone();
        if let Some(obj) = meta.as_object_mut() {
            obj.remove("draw_offer");
        }

        let final_row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET meta = $2
             WHERE id = $1
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(row.id)
        .bind(meta)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        let (user_earned, user_balance) = if is_white {
            (completion.gold.white_earned, white_balance)
        } else {
            (completion.gold.black_earned, black_balance)
        };

        Ok(ResignSuccess {
            game: final_row,
            result: "draw".to_string(),
            termination: "agreement".to_string(),
            ratings: completion.ratings,
            user_gold_earned: user_earned,
            user_gold_balance: user_balance,
        })
    }

    /// Opponent-flag claim fallback. Server verifies against clock truth.
    pub async fn claim_timeout(
        &self,
        public_id: &str,
        claimant_id: i64,
    ) -> Result<ResignSuccess, GameServiceError> {
        let mut tx = self.pool.begin().await?;

        let game = sqlx::query_as::<_, GameRow>(
            "SELECT id, public_id, white_user_id, black_user_id, pool, time_control,
                    rated, status, result, termination, fen_start, fen_current,
                    move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                    version, created_at, started_at, ended_at, meta
             FROM games
             WHERE public_id = $1
             FOR UPDATE",
        )
        .bind(public_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| GameServiceError::NotFound(public_id.to_string()))?;

        let is_white = claimant_id == game.white_user_id;
        let is_black = claimant_id == game.black_user_id;
        if !is_white && !is_black {
            return Err(GameServiceError::NotParticipant);
        }

        if game.status == "finished" {
            let user_balance: i64 =
                sqlx::query_scalar("SELECT balance FROM gold_balances WHERE user_id = $1")
                    .bind(claimant_id)
                    .fetch_optional(&mut *tx)
                    .await?
                    .unwrap_or(0);
            let earned = match game.result.as_deref() {
                Some("white") if is_white => 25,
                Some("black") if is_black => 25,
                Some("draw") => 10,
                _ => 5,
            };
            tx.rollback().await?;
            return Ok(ResignSuccess {
                game: game.clone(),
                result: game.result.clone().unwrap_or_default(),
                termination: game.termination.clone().unwrap_or_default(),
                ratings: None,
                user_gold_earned: earned,
                user_gold_balance: user_balance,
            });
        }

        if game.status != "active" {
            return Err(GameServiceError::GameFinished(game.status));
        }

        let engine = RulesEngine::from_fen(&game.fen_current)
            .map_err(|e| GameServiceError::IllegalMove(e.to_string()))?;
        let to_move = engine.to_move();
        let opponent_color = if is_white { Color::Black } else { Color::White };
        let claimant_color = if is_white { Color::White } else { Color::Black };

        if to_move != opponent_color {
            return Err(GameServiceError::NotOpponentTurn);
        }

        let now = time::OffsetDateTime::now_utc();
        let now_ms = (now.unix_timestamp_nanos() / 1_000_000) as i64;
        let last_tick_ms = game
            .last_tick_ts
            .map(|t| (t.unix_timestamp_nanos() / 1_000_000) as i64)
            .unwrap_or(now_ms);
        let elapsed_ms = now_ms.saturating_sub(last_tick_ms);

        let opponent_remaining_ms = if is_white {
            game.b_remaining_ms.unwrap_or(0)
        } else {
            game.w_remaining_ms.unwrap_or(0)
        };

        if opponent_remaining_ms > elapsed_ms {
            return Err(GameServiceError::ClockNotExpired);
        }

        let opponent_can_mate =
            RulesEngine::can_potentially_mate(engine.board(), claimant_color, opponent_color);
        let (result, termination) = if opponent_can_mate {
            (if is_white { "white" } else { "black" }, "timeout")
        } else {
            ("draw", "timeout")
        };

        let tc_initial_ms = game
            .time_control
            .get("initial_ms")
            .and_then(|v| v.as_i64())
            .unwrap_or(180_000);

        let (w_remaining, b_remaining) = if is_white {
            (game.w_remaining_ms.unwrap_or(tc_initial_ms), 0)
        } else {
            (0, game.b_remaining_ms.unwrap_or(tc_initial_ms))
        };

        let (row, completion, (white_balance, black_balance)) = Self::finalize_game_in_tx(
            &mut tx,
            &game,
            FinalizeParams {
                result,
                termination,
                ended_at: now,
                final_fen: &game.fen_current,
                move_count: game.move_count,
                version: game.version + 1,
                w_remaining_ms: w_remaining,
                b_remaining_ms: b_remaining,
                last_tick_ts: now,
            },
        )
        .await?;

        tx.commit().await?;

        let (user_earned, user_balance) = if is_white {
            (completion.gold.white_earned, white_balance)
        } else {
            (completion.gold.black_earned, black_balance)
        };

        Ok(ResignSuccess {
            game: row,
            result: result.to_string(),
            termination: termination.to_string(),
            ratings: completion.ratings,
            user_gold_earned: user_earned,
            user_gold_balance: user_balance,
        })
    }

    /// Finalize game outcome in a single transaction (ADR-001, ADR-005, ADR-006, lifecycle §2.4).
    async fn finalize_game_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        game: &GameRow,
        params: FinalizeParams<'_>,
    ) -> Result<(GameRow, GameCompletionData, (i64, i64)), GameServiceError> {
        let row = sqlx::query_as::<_, GameRow>(
            "UPDATE games
             SET status = 'finished', result = $2, termination = $3, ended_at = $4,
                 fen_current = $5, move_count = $6, version = $7,
                 w_remaining_ms = $8, b_remaining_ms = $9, last_tick_ts = $10,
                 started_at = coalesce(started_at, $10)
             WHERE id = $1
             RETURNING id, public_id, white_user_id, black_user_id, pool, time_control,
                       rated, status, result, termination, fen_start, fen_current,
                       move_count, w_remaining_ms, b_remaining_ms, last_tick_ts,
                       version, created_at, started_at, ended_at, meta",
        )
        .bind(game.id)
        .bind(params.result)
        .bind(params.termination)
        .bind(params.ended_at)
        .bind(params.final_fen)
        .bind(params.move_count)
        .bind(params.version)
        .bind(params.w_remaining_ms)
        .bind(params.b_remaining_ms)
        .bind(params.last_tick_ts)
        .fetch_one(&mut **tx)
        .await?;

        let ratings_dto = if game.rated {
            let outcome = match params.result {
                "white" => Outcome::WhiteWin,
                "black" => Outcome::BlackWin,
                _ => Outcome::Draw,
            };
            let pool = Pool::parse(&game.pool).unwrap_or(Pool::Blitz);
            let applier = PgRatingApplier::new();
            let applied = applier
                .apply_in_tx(
                    tx,
                    game.id,
                    game.white_user_id,
                    game.black_user_id,
                    pool,
                    outcome,
                )
                .await?;
            applied.map(|a| GameRatings {
                white: RatingDeltaDto {
                    before: a.white.before,
                    after: a.white.after,
                    delta: a.white.delta,
                },
                black: RatingDeltaDto {
                    before: a.black.before,
                    after: a.black.after,
                    delta: a.black.delta,
                },
            })
        } else {
            None
        };

        let (white_gold, black_gold) = match params.result {
            "white" => (25, 5),
            "black" => (5, 25),
            _ => (10, 10),
        };

        let white_op = LedgerOp {
            user_id: game.white_user_id,
            delta: white_gold,
            reason: GoldReason::Gameplay,
            ref_type: Some("game".to_string()),
            ref_id: Some(game.public_id.clone()),
            idem_key: Some(format!("game:{}:gold:{}", game.id, game.white_user_id)),
            meta: serde_json::json!({ "game_id": game.id, "result": params.result }),
        };
        let white_res = PgGoldLedger::apply_in_tx(tx, &white_op).await?;

        let black_op = LedgerOp {
            user_id: game.black_user_id,
            delta: black_gold,
            reason: GoldReason::Gameplay,
            ref_type: Some("game".to_string()),
            ref_id: Some(game.public_id.clone()),
            idem_key: Some(format!("game:{}:gold:{}", game.id, game.black_user_id)),
            meta: serde_json::json!({ "game_id": game.id, "result": params.result }),
        };
        let black_res = PgGoldLedger::apply_in_tx(tx, &black_op).await?;

        let white_balance = match white_res {
            GrantResult::Applied { balance_after, .. } => balance_after,
            GrantResult::Duplicate { balance_after } => balance_after,
        };
        let black_balance = match black_res {
            GrantResult::Applied { balance_after, .. } => balance_after,
            GrantResult::Duplicate { balance_after } => balance_after,
        };

        let completion = GameCompletionData {
            result: params.result.to_string(),
            termination: params.termination.to_string(),
            ratings: ratings_dto,
            gold: GameGoldEarnings {
                white_earned: white_gold,
                black_earned: black_gold,
            },
        };

        Ok((row, completion, (white_balance, black_balance)))
    }
}

struct FinalizeParams<'a> {
    result: &'a str,
    termination: &'a str,
    ended_at: time::OffsetDateTime,
    final_fen: &'a str,
    move_count: i32,
    version: i32,
    w_remaining_ms: i64,
    b_remaining_ms: i64,
    last_tick_ts: time::OffsetDateTime,
}
