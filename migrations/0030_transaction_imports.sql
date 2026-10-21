-- Financial history is imported explicitly; never recreate stock or operational orders.
CREATE TABLE transaction_import_batches(
 id uuid PRIMARY KEY,kind text NOT NULL CHECK(kind IN ('customer_invoices','supplier_bills','customer_payments','supplier_payments','journals')),
 csv text NOT NULL CHECK(octet_length(csv)<=262144),delimiter text NOT NULL CHECK(delimiter IN (',',';')),
 preview jsonb NOT NULL,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 committed_at timestamptz,committed_result jsonb,
 CHECK((committed_at IS NULL)=(committed_result IS NULL))
);
CREATE INDEX transaction_import_history ON transaction_import_batches(created_at DESC,id);
CREATE FUNCTION guard_transaction_import_batch() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
 IF OLD.committed_at IS NOT NULL OR NEW.committed_at IS NULL OR
 (to_jsonb(NEW)-'committed_at'-'committed_result') IS DISTINCT FROM (to_jsonb(OLD)-'committed_at'-'committed_result')
 THEN RAISE EXCEPTION 'Import previews are immutable; commit once or create a new preview'; END IF;
 RETURN NEW; END $$;
CREATE TRIGGER transaction_import_batch_update BEFORE UPDATE ON transaction_import_batches FOR EACH ROW EXECUTE FUNCTION guard_transaction_import_batch();
CREATE TRIGGER transaction_import_batch_delete BEFORE DELETE ON transaction_import_batches FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER transaction_import_batch_truncate BEFORE TRUNCATE ON transaction_import_batches FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TABLE historical_documents(
 id uuid PRIMARY KEY,side text NOT NULL CHECK(side IN ('customer','supplier')),reference text NOT NULL CHECK(char_length(reference) BETWEEN 1 AND 100),
 party_id uuid NOT NULL,customer_id uuid GENERATED ALWAYS AS(CASE WHEN side='customer' THEN party_id END) STORED REFERENCES customers(id),
 supplier_id uuid GENERATED ALWAYS AS(CASE WHEN side='supplier' THEN party_id END) STORED REFERENCES suppliers(id),
 total numeric(18,2) NOT NULL CHECK(total>0 AND total<=1000000000000),counter_account text NOT NULL REFERENCES accounts(code),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),document_date date NOT NULL,due_date date NOT NULL CHECK(due_date>=document_date),
 description text NOT NULL,created_at timestamptz NOT NULL,recorded_at timestamptz NOT NULL DEFAULT now(),created_by uuid NOT NULL REFERENCES users(id),
 batch_id uuid NOT NULL REFERENCES transaction_import_batches(id),UNIQUE(side,reference),UNIQUE(id,side,party_id)
);
CREATE TABLE historical_payments(
 id uuid PRIMARY KEY,document_id uuid NOT NULL REFERENCES historical_documents(id),reference text NOT NULL UNIQUE CHECK(char_length(reference) BETWEEN 1 AND 100),
 amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),cash_account_code text NOT NULL REFERENCES treasury_accounts(account_code),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),payment_date date NOT NULL,created_at timestamptz NOT NULL,recorded_at timestamptz NOT NULL DEFAULT now(),
 created_by uuid NOT NULL REFERENCES users(id),batch_id uuid NOT NULL REFERENCES transaction_import_batches(id)
);
CREATE TABLE historical_reversals(
 id uuid PRIMARY KEY,document_id uuid UNIQUE REFERENCES historical_documents(id),payment_id uuid UNIQUE REFERENCES historical_payments(id),
 CHECK((document_id IS NULL)<>(payment_id IS NULL)),reason text NOT NULL CHECK(char_length(reason) BETWEEN 3 AND 1000),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),created_at timestamptz NOT NULL DEFAULT now(),created_by uuid NOT NULL REFERENCES users(id)
);
CREATE TABLE historical_import_journals(
 id uuid PRIMARY KEY,reference text NOT NULL UNIQUE,posting_date date NOT NULL,journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
 batch_id uuid NOT NULL REFERENCES transaction_import_batches(id),created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now()
);
DO $$ DECLARE t text; BEGIN FOREACH t IN ARRAY ARRAY['historical_documents','historical_payments','historical_reversals','historical_import_journals'] LOOP
 EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',t||'_immutable',t);
 EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',t||'_truncate',t);
END LOOP; END $$;
ALTER TABLE party_allocations DROP CONSTRAINT party_allocations_target_type_check;
ALTER TABLE party_allocations ADD CONSTRAINT party_allocations_target_type_check CHECK(target_type IN ('invoice','bill','opening_receivable','opening_payable','historical_receivable','historical_payable'));
CREATE FUNCTION historical_document_paid(target uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
 SELECT COALESCE((SELECT sum(p.amount) FROM historical_payments p WHERE p.document_id=target AND NOT EXISTS(SELECT 1 FROM historical_reversals r WHERE r.payment_id=p.id)),0)
 +COALESCE((SELECT sum(a.amount) FROM party_allocations a WHERE a.target_id=target AND a.target_type IN ('historical_receivable','historical_payable') AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_allocation' AND r.source_id=a.id)),0)
$$;
-- Expose logical dates and signed events for the party subledger. Counter amounts
-- are positive when receivable/payable grows, negative when it is settled.
CREATE VIEW historical_party_events AS
 SELECT d.side,d.party_id,d.id document_id,d.reference number,d.created_at,'historical_document'::text kind,d.id source_id,d.total amount,d.journal_id FROM historical_documents d
 UNION ALL SELECT d.side,d.party_id,d.id,d.reference,p.created_at,'historical_payment',p.id,-p.amount,p.journal_id FROM historical_payments p JOIN historical_documents d ON d.id=p.document_id
 UNION ALL SELECT d.side,d.party_id,d.id,d.reference,r.created_at,'historical_document_reversal',r.id,-d.total,r.journal_id FROM historical_reversals r JOIN historical_documents d ON d.id=r.document_id
 UNION ALL SELECT d.side,d.party_id,d.id,d.reference,r.created_at,'historical_payment_reversal',r.id,p.amount,r.journal_id FROM historical_reversals r JOIN historical_payments p ON p.id=r.payment_id JOIN historical_documents d ON d.id=p.document_id;
DO $$ DECLARE c text; definition text; BEGIN
 SELECT pg_get_constraintdef(oid) INTO definition FROM pg_constraint WHERE conrelid='journal_entries'::regclass AND conname='journal_entries_source_type_check';
 ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
 SELECT string_agg(quote_literal(v),',') INTO c FROM (SELECT DISTINCT m[1] v FROM regexp_matches(definition,'''([^'']+)''','g') m UNION SELECT unnest(ARRAY['historical_document','historical_payment','historical_reversal'])) x;
 EXECUTE 'ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check CHECK(source_type IN ('||c||'))';
END $$;

CREATE UNIQUE INDEX historical_document_reference_folded ON historical_documents(side,lower(reference));
CREATE UNIQUE INDEX historical_payment_reference_folded ON historical_payments(lower(reference));
CREATE UNIQUE INDEX historical_journal_reference_folded ON historical_import_journals(lower(reference));
