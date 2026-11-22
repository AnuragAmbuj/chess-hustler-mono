-- 0005_daily_claims_claim_date_ist.sql — ensure daily_claims has claim_date_ist with unique constraint

ALTER TABLE daily_claims ADD COLUMN IF NOT EXISTS claim_date_ist date;

UPDATE daily_claims SET claim_date_ist = ist_date WHERE claim_date_ist IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS daily_claims_user_claim_date_ist_uidx ON daily_claims (user_id, claim_date_ist);

CREATE OR REPLACE FUNCTION sync_daily_claims_dates()
RETURNS TRIGGER AS $$
BEGIN
  IF NEW.claim_date_ist IS NULL AND NEW.ist_date IS NOT NULL THEN
    NEW.claim_date_ist := NEW.ist_date;
  ELSIF NEW.ist_date IS NULL AND NEW.claim_date_ist IS NOT NULL THEN
    NEW.ist_date := NEW.claim_date_ist;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sync_daily_claims_dates ON daily_claims;

CREATE TRIGGER trg_sync_daily_claims_dates
BEFORE INSERT OR UPDATE ON daily_claims
FOR EACH ROW
EXECUTE FUNCTION sync_daily_claims_dates();
