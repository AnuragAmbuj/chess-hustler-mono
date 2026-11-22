//! Anti-cheat offline game analyzer (ADR-014).
//!
//! Analyzes completed games asynchronously:
//! 1. Move timing regularity (uniform think time / robotic interval detection).
//! 2. Tactical engine agreement heuristic (high correlation with optimal tactical moves).
//! 3. Pair-farming detection (repeated quick resignations between same players).
//! 4. Draw-farming detection (repeated early draws between same players).
//!
//! Emits findings to `anomaly_flags` with status='open' for human-in-the-loop review.
//! Strictly conforms to ADR-014: NO automated bans based on statistical detection alone.

use crate::error::ModerationError;
use crate::models::AnomalyFlagItem;
use serde_json::json;
use shakmaty::uci::UciMove;
use shakmaty::{Chess, Color, Move, Position, Role, Square};
use sqlx::{PgPool, Row};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct AntiCheatAnalyzer {
    pool: PgPool,
}

impl AntiCheatAnalyzer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Analyze a completed game by its internal bigint ID.
    pub async fn analyze_game(
        &self,
        game_id: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        let game_row = sqlx::query(
            r#"
            SELECT id, public_id, white_user_id, black_user_id, status, result, termination, move_count, created_at
            FROM games
            WHERE id = $1
            "#,
        )
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        let game = match game_row {
            Some(row) => row,
            None => return Ok(vec![]),
        };

        let status: String = game.get("status");
        if status != "finished" {
            return Ok(vec![]);
        }

        let white_user_id: i64 = game.get("white_user_id");
        let black_user_id: i64 = game.get("black_user_id");

        let mut flags = Vec::new();

        // 1. Move timing regularity
        let timing_flags = self
            .detect_move_timing_regularity(game_id, white_user_id, black_user_id)
            .await?;
        flags.extend(timing_flags);

        // 2. Engine agreement / tactical move correlation
        let engine_flags = self
            .detect_engine_agreement(game_id, white_user_id, black_user_id)
            .await?;
        flags.extend(engine_flags);

        // 3. Pair farming & draw farming
        let pair_flags = self
            .detect_pair_and_draw_farming(game_id, white_user_id, black_user_id)
            .await?;
        flags.extend(pair_flags);

        Ok(flags)
    }

    /// Detector 1: Move timing regularity.
    /// Robotic players take nearly identical time per move regardless of position complexity.
    /// If standard deviation of think times is < 0.25s over 8+ moves, flag kind='move_time'.
    async fn detect_move_timing_regularity(
        &self,
        game_id: i64,
        white_user_id: i64,
        black_user_id: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT ply, ts
            FROM moves
            WHERE game_id = $1
            ORDER BY ply ASC
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        if rows.len() < 16 {
            // Need sufficient moves for statistical power
            return Ok(vec![]);
        }

        let mut white_ts = Vec::new();
        let mut black_ts = Vec::new();

        for row in rows {
            let ply: i32 = row.get("ply");
            let ts: OffsetDateTime = row.get("ts");
            if ply % 2 == 1 {
                white_ts.push(ts);
            } else {
                black_ts.push(ts);
            }
        }

        let mut flags = Vec::new();

        for (user_id, color_str, timestamps) in [
            (white_user_id, "white", white_ts),
            (black_user_id, "black", black_ts),
        ] {
            if timestamps.len() < 8 {
                continue;
            }

            // Calculate think times: delta between move i and move i-1
            let mut intervals = Vec::new();
            for i in 1..timestamps.len() {
                let diff = timestamps[i] - timestamps[i - 1];
                let s = diff.as_seconds_f64();
                if s > 0.05 && s < 180.0 {
                    intervals.push(s);
                }
            }

            if intervals.len() < 7 {
                continue;
            }

            let n = intervals.len() as f64;
            let mean = intervals.iter().sum::<f64>() / n;
            let variance = intervals.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / n;
            let std_dev = variance.sqrt();

            // Human play standard deviation is typically 1.5s - 10s+.
            // An std_dev < 0.25s is extraordinarily robotic cadence.
            if std_dev < 0.25 {
                let score = ((0.75 + (0.25 - std_dev) * 1.0).clamp(0.75, 0.99)) as f32;
                let evidence = json!({
                    "game_id": game_id,
                    "color": color_str,
                    "move_count": intervals.len(),
                    "mean_s": (mean * 1000.0).round() / 1000.0,
                    "std_dev_s": (std_dev * 1000.0).round() / 1000.0,
                    "intervals_s": intervals.iter().take(10).map(|x| (x * 100.0).round() / 100.0).collect::<Vec<_>>()
                });

                if let Some(flag) = self
                    .insert_flag_if_not_exists("move_time", user_id, None, score, evidence, game_id)
                    .await?
                {
                    flags.push(flag);
                }
            }
        }

        Ok(flags)
    }

    /// Detector 2: Engine agreement & tactical move correlation.
    /// Evaluates non-book moves using Shakmaty tactical heuristics.
    /// If a player achieves >= 90% top-move agreement over 8+ evaluated plies, flag kind='engine_match'.
    async fn detect_engine_agreement(
        &self,
        game_id: i64,
        white_user_id: i64,
        black_user_id: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT ply, uci
            FROM moves
            WHERE game_id = $1
            ORDER BY ply ASC
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        if rows.len() < 14 {
            return Ok(vec![]);
        }

        let mut pos = Chess::default();
        let mut white_matches = 0usize;
        let mut white_evaled = 0usize;
        let mut black_matches = 0usize;
        let mut black_evaled = 0usize;

        for row in rows {
            let ply: i32 = row.get("ply");
            let uci_str: String = row.get("uci");

            let uci_move = match UciMove::from_ascii(uci_str.as_bytes()) {
                Ok(m) => m,
                Err(_) => break,
            };

            let legal_move = match uci_move.to_move(&pos) {
                Ok(m) => m,
                Err(_) => break,
            };

            // Evaluate candidate positions after ply 6 (avoid opening book distortion)
            if ply >= 7 {
                let is_white = ply % 2 == 1;
                let (best_move, _) = evaluate_best_move(&pos);

                if let Some(best) = best_move {
                    let is_match = best == legal_move;
                    if is_white {
                        white_evaled += 1;
                        if is_match {
                            white_matches += 1;
                        }
                    } else {
                        black_evaled += 1;
                        if is_match {
                            black_matches += 1;
                        }
                    }
                }
            }

            pos.play_unchecked(legal_move);
        }

        let mut flags = Vec::new();

        for (user_id, color_str, evaled, matches) in [
            (white_user_id, "white", white_evaled, white_matches),
            (black_user_id, "black", black_evaled, black_matches),
        ] {
            if evaled >= 8 {
                let agreement_ratio = matches as f32 / evaled as f32;
                if agreement_ratio >= 0.88 {
                    let score = (0.80 + (agreement_ratio - 0.88) * 1.6).clamp(0.80, 0.99);
                    let evidence = json!({
                        "game_id": game_id,
                        "color": color_str,
                        "evaluated_moves": evaled,
                        "top_matches": matches,
                        "agreement_pct": (agreement_ratio * 100.0).round()
                    });

                    if let Some(flag) = self
                        .insert_flag_if_not_exists(
                            "engine_match",
                            user_id,
                            None,
                            score,
                            evidence,
                            game_id,
                        )
                        .await?
                    {
                        flags.push(flag);
                    }
                }
            }
        }

        Ok(flags)
    }

    /// Detector 3 & 4: Pair farming & draw farming.
    /// Detects repeated pairings, quick reciprocal resignations, or collusion between two users.
    async fn detect_pair_and_draw_farming(
        &self,
        game_id: i64,
        white_user_id: i64,
        black_user_id: i64,
    ) -> Result<Vec<AnomalyFlagItem>, ModerationError> {
        let rows = sqlx::query(
            r#"
            SELECT id, white_user_id, black_user_id, result, termination, move_count
            FROM games
            WHERE ((white_user_id = $1 AND black_user_id = $2)
                OR (white_user_id = $2 AND black_user_id = $1))
              AND status = 'finished'
              AND created_at > now() - interval '24 hours'
            ORDER BY created_at DESC
            LIMIT 20
            "#,
        )
        .bind(white_user_id)
        .bind(black_user_id)
        .fetch_all(&self.pool)
        .await?;

        if rows.len() < 3 {
            return Ok(vec![]);
        }

        let mut flags = Vec::new();
        let mut quick_resigns_white_won = 0usize;
        let mut quick_resigns_black_won = 0usize;
        let mut quick_draws = 0usize;

        for r in &rows {
            let res: Option<String> = r.get("result");
            let term: Option<String> = r.get("termination");
            let plies: i32 = r.get("move_count");
            let w_uid: i64 = r.get("white_user_id");

            if term.as_deref() == Some("resignation") && plies <= 6 {
                if let Some(result_str) = res {
                    if (result_str == "white" && w_uid == white_user_id)
                        || (result_str == "black" && w_uid != white_user_id)
                    {
                        quick_resigns_white_won += 1;
                    } else {
                        quick_resigns_black_won += 1;
                    }
                }
            } else if res.as_deref() == Some("draw") && plies <= 8 {
                quick_draws += 1;
            }
        }

        // Pair farming: one player won >= 2 quick resignations against the same opponent
        if quick_resigns_white_won >= 2 {
            let score = (0.85 + 0.05 * quick_resigns_white_won as f32).clamp(0.85, 0.99);
            let evidence = json!({
                "recent_paired_games": rows.len(),
                "quick_resigns": quick_resigns_white_won,
                "beneficiary_user_id": white_user_id,
                "donor_user_id": black_user_id,
                "game_id": game_id
            });
            if let Some(f) = self
                .insert_flag_if_not_exists(
                    "pair_farming",
                    white_user_id,
                    Some(black_user_id),
                    score,
                    evidence,
                    game_id,
                )
                .await?
            {
                flags.push(f);
            }
        } else if quick_resigns_black_won >= 2 {
            let score = (0.85 + 0.05 * quick_resigns_black_won as f32).clamp(0.85, 0.99);
            let evidence = json!({
                "recent_paired_games": rows.len(),
                "quick_resigns": quick_resigns_black_won,
                "beneficiary_user_id": black_user_id,
                "donor_user_id": white_user_id,
                "game_id": game_id
            });
            if let Some(f) = self
                .insert_flag_if_not_exists(
                    "pair_farming",
                    black_user_id,
                    Some(white_user_id),
                    score,
                    evidence,
                    game_id,
                )
                .await?
            {
                flags.push(f);
            }
        }

        // Draw farming: >= 3 quick draws
        if quick_draws >= 3 {
            let score = (0.80 + 0.05 * quick_draws as f32).clamp(0.80, 0.98);
            let evidence = json!({
                "recent_paired_games": rows.len(),
                "quick_draws": quick_draws,
                "game_id": game_id
            });
            if let Some(f) = self
                .insert_flag_if_not_exists(
                    "draw_farming",
                    white_user_id,
                    Some(black_user_id),
                    score,
                    evidence,
                    game_id,
                )
                .await?
            {
                flags.push(f);
            }
        }

        Ok(flags)
    }

    /// Idempotent flag insertion: skips if this game_id has already been flagged for this kind and subject.
    async fn insert_flag_if_not_exists(
        &self,
        kind: &str,
        subject_user_id: i64,
        related_user_id: Option<i64>,
        score: f32,
        evidence: serde_json::Value,
        game_id: i64,
    ) -> Result<Option<AnomalyFlagItem>, ModerationError> {
        let existing = sqlx::query(
            r#"
            SELECT id FROM anomaly_flags
            WHERE kind = $1
              AND subject_user_id = $2
              AND (evidence->>'game_id')::bigint = $3
            LIMIT 1
            "#,
        )
        .bind(kind)
        .bind(subject_user_id)
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Ok(None);
        }

        let row = sqlx::query(
            r#"
            INSERT INTO anomaly_flags (kind, subject_user_id, related_user_id, score, evidence, status)
            VALUES ($1, $2, $3, $4, $5, 'open')
            RETURNING id, kind, subject_user_id, related_user_id, score, evidence, status, reviewed_by, reviewed_at, created_at
            "#,
        )
        .bind(kind)
        .bind(subject_user_id)
        .bind(related_user_id)
        .bind(score)
        .bind(evidence)
        .fetch_one(&self.pool)
        .await?;

        let created_at: OffsetDateTime = row.get("created_at");
        let reviewed_at: Option<OffsetDateTime> = row.get("reviewed_at");

        Ok(Some(AnomalyFlagItem {
            id: row.get("id"),
            kind: row.get("kind"),
            subject_user_id: row.get("subject_user_id"),
            subject_handle: None,
            related_user_id: row.get("related_user_id"),
            related_handle: None,
            evidence: row.get("evidence"),
            score: row.get("score"),
            status: row.get("status"),
            reviewed_by: row.get("reviewed_by"),
            reviewed_at: reviewed_at.map(|t| t.format(&Rfc3339).unwrap_or_default()),
            created_at: created_at.format(&Rfc3339).unwrap_or_default(),
        }))
    }
}

/// Simple positional evaluator:
/// Evaluates material + center control + mobility to pick the best tactical candidate.
fn evaluate_best_move(pos: &Chess) -> (Option<Move>, i32) {
    let legal = pos.legal_moves();
    if legal.is_empty() {
        return (None, 0);
    }

    let turn = pos.turn();
    let mut best_move = None;
    let mut best_score = i32::MIN;

    for m in legal {
        let mut next = pos.clone();
        next.play_unchecked(m);

        let score = evaluate_position(&next, turn);
        if score > best_score {
            best_score = score;
            best_move = Some(m);
        }
    }

    (best_move, best_score)
}

/// Material balance + center control + piece mobility score relative to `perspective`.
fn evaluate_position(pos: &Chess, perspective: Color) -> i32 {
    let board = pos.board();
    let mut score = 0i32;

    for sq in Square::ALL {
        if let Some(piece) = board.piece_at(sq) {
            let val = match piece.role {
                Role::Pawn => 100,
                Role::Knight => 320,
                Role::Bishop => 330,
                Role::Rook => 500,
                Role::Queen => 900,
                Role::King => 20000,
            };

            // Small center square bonus for pawns and knights
            let center_bonus = match sq {
                Square::D4 | Square::E4 | Square::D5 | Square::E5 => 20,
                Square::C3
                | Square::C4
                | Square::C5
                | Square::C6
                | Square::F3
                | Square::F4
                | Square::F5
                | Square::F6 => 10,
                _ => 0,
            };

            let piece_val = val + center_bonus;
            if piece.color == perspective {
                score += piece_val;
            } else {
                score -= piece_val;
            }
        }
    }

    // Add mobility bonus
    let mobility = pos.legal_moves().len() as i32;
    if pos.turn() == perspective {
        score += mobility * 2;
    } else {
        score -= mobility * 2;
    }

    score
}
