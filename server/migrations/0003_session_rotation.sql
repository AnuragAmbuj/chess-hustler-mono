-- 0003_session_rotation.sql — refresh-token rotation bookkeeping (ADR-008).
-- Rotated tokens are kept (hash retained) so REUSE of a superseded refresh
-- token can be detected → the whole session family is revoked
-- (revoked_reason = 'rotation_reuse'). 'rotated' marks superseded tokens.

ALTER TABLE user_sessions
  DROP CONSTRAINT user_sessions_revoked_reason_check;
ALTER TABLE user_sessions
  ADD CONSTRAINT user_sessions_revoked_reason_check
  CHECK (revoked_reason IN ('logout','logout_all','rotation_reuse','admin','rotated'));
