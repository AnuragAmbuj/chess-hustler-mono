-- 0002_finalize_idempotency.sql — T3 replay wall (schema §14, lifecycle I-5).
-- One rating-history row per (user, game): a repeated finalize attempt is
-- detected by the applier and skipped instead of double-applying Glicko-2.

CREATE UNIQUE INDEX IF NOT EXISTS rating_history_user_game_uidx
  ON rating_history (user_id, game_id);
