-- A logical dump preserves posting_transaction as ordinary data, while the
-- destination cluster allocates transaction identifiers independently. Require
-- the header's physical insertion transaction too, so a coincidentally reused
-- source transaction ID cannot permit appending lines after a restore.
CREATE OR REPLACE FUNCTION guard_journal_append() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_transaction xid8 := pg_current_xact_id();
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM journal_entries
        WHERE id=NEW.journal_id
          AND posting_transaction=current_transaction
          AND xmin::text::numeric=mod(current_transaction::text::numeric,4294967296)
    ) THEN
        RAISE EXCEPTION 'Cannot append lines to a posted journal';
    END IF;
    RETURN NEW;
END $$;
