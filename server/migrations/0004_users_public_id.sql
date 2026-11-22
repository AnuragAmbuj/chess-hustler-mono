-- 0004_users_public_id.sql — public user identifier (rest.md §1: "Public
-- ULIDs; internal bigints never serialized"). `public_id` is the ONLY user id
-- clients ever see; bigint ids stay internal (joins, FKs, JWT `sub` claims).
-- Existing rows get a deterministic, immutable value; new signups mint a ULID.

ALTER TABLE users ADD COLUMN public_id text;

UPDATE users SET public_id = 'u' || lpad(to_hex(id::bigint), 16, '0');

ALTER TABLE users ALTER COLUMN public_id SET NOT NULL;

CREATE UNIQUE INDEX users_public_id_uidx ON users (public_id);
