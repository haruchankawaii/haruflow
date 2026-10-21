-- Preserve ledger dates and amounts. The new timestamp records insertion time
-- separately when an explicitly dated financial journal is posted.
ALTER TABLE journal_entries ADD COLUMN posted_at timestamptz NOT NULL DEFAULT now();
ALTER TABLE journal_entries ADD COLUMN reporting_kind text NOT NULL DEFAULT 'normal' CHECK(reporting_kind IN ('normal','year_close'));
ALTER TABLE journal_entries DISABLE TRIGGER immutable_journal;
UPDATE journal_entries SET posted_at=created_at;
ALTER TABLE journal_entries ENABLE TRIGGER immutable_journal;
ALTER TABLE accounting_period ADD COLUMN version bigint NOT NULL DEFAULT 1 CHECK(version>0);
DO $$
DECLARE chosen text; expression text;
BEGIN
 SELECT n::text INTO chosen FROM generate_series(3100,3199)n WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1;
 IF chosen IS NULL THEN
  WITH candidates AS(SELECT 1000000000::bigint n UNION SELECT code::bigint+1 FROM accounts WHERE code~'^[1-9][0-9]{9}$' AND code<>'9999999999')
  SELECT min(n)::text INTO chosen FROM candidates WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text);
 END IF;
 IF chosen IS NULL THEN RAISE EXCEPTION 'No unused retained earnings account code'; END IF;
 INSERT INTO accounts(code,name,kind,system_role) VALUES(chosen,'Saldo laba tahun sebelumnya','equity','retained_earnings');
 SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint WHERE conrelid='journal_entries'::regclass AND conname='journal_entries_source_type_check';
 ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
 EXECUTE format('ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check CHECK ((%s) OR source_type=''year_close'')',expression);
END $$;
CREATE TABLE period_events(
 id uuid PRIMARY KEY, action text NOT NULL CHECK(action IN ('close','reopen','year_close','year_close_reversal')),
 previous_closed_through date,closed_through date,reason text NOT NULL CHECK(length(reason) BETWEEN 3 AND 1000),
 version bigint NOT NULL UNIQUE CHECK(version>1),created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE fiscal_closings(
 id uuid PRIMARY KEY,year integer NOT NULL CHECK(year BETWEEN 1 AND 9998),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
 previous_closed_through date,net_profit numeric(18,2) NOT NULL,
 reason text NOT NULL CHECK(length(reason) BETWEEN 3 AND 1000),
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE fiscal_closing_reversals(
 id uuid PRIMARY KEY,closing_id uuid NOT NULL UNIQUE REFERENCES fiscal_closings(id),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),reason text NOT NULL CHECK(length(reason) BETWEEN 3 AND 1000),
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
DO $$ DECLARE target text; BEGIN
 FOREACH target IN ARRAY ARRAY['period_events','fiscal_closings','fiscal_closing_reversals'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',target||'_immutable',target);
  EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',target||'_no_truncate',target);
 END LOOP;
END $$;
