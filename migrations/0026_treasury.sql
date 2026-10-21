-- Registration never rewrites legacy cash journals or assigns new meaning to old accounts.
CREATE TABLE treasury_accounts(
 account_code text PRIMARY KEY REFERENCES accounts(code),kind text NOT NULL CHECK(kind IN ('cash','bank')),
 bank_name text,account_number text,currency text NOT NULL DEFAULT 'IDR' CHECK(currency='IDR'),
 created_by uuid REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
INSERT INTO treasury_accounts(account_code,kind) VALUES('1100','cash');
CREATE TABLE treasury_transfers(
 id uuid PRIMARY KEY,number text NOT NULL UNIQUE,from_account text NOT NULL REFERENCES treasury_accounts(account_code),
 to_account text NOT NULL REFERENCES treasury_accounts(account_code),amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 reference text,reason text NOT NULL,journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),CHECK(from_account<>to_account));
CREATE SEQUENCE treasury_transfer_number;
DO $$ DECLARE expression text; name text; BEGIN
 FOREACH name IN ARRAY ARRAY['journal_entries','posting_reversals'] LOOP
 SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint WHERE conrelid=name::regclass AND conname=name||'_source_type_check';
 EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I',name,name||'_source_type_check');
 EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I CHECK ((%s) OR source_type=''treasury_transfer'')',name,name||'_source_type_check',expression);
 END LOOP;
END $$;
CREATE TABLE bank_statements(
 id uuid PRIMARY KEY,account_code text NOT NULL REFERENCES treasury_accounts(account_code),reference text NOT NULL,
 from_date date NOT NULL,to_date date NOT NULL,opening_balance numeric(18,2) NOT NULL,closing_balance numeric(18,2) NOT NULL,
 file_sha256 text NOT NULL CHECK(length(file_sha256)=64),created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(account_code,reference),UNIQUE(account_code,file_sha256),CHECK(from_date<=to_date));
CREATE TABLE bank_statement_lines(
 id uuid PRIMARY KEY,statement_id uuid NOT NULL REFERENCES bank_statements(id),row_number integer NOT NULL CHECK(row_number>0),
 transaction_date date NOT NULL,reference text NOT NULL,description text NOT NULL,amount numeric(18,2) NOT NULL CHECK(amount<>0 AND abs(amount)<=1000000000000),
 UNIQUE(statement_id,row_number),UNIQUE(statement_id,reference));
CREATE TABLE bank_line_versions(line_id uuid PRIMARY KEY REFERENCES bank_statement_lines(id),version bigint NOT NULL DEFAULT 1 CHECK(version>0));
CREATE TABLE bank_reconciliation_matches(
 line_id uuid PRIMARY KEY REFERENCES bank_statement_lines(id),journal_line_id uuid NOT NULL UNIQUE REFERENCES journal_lines(id),
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE bank_reconciliation_events(
 id uuid PRIMARY KEY,line_id uuid NOT NULL REFERENCES bank_statement_lines(id),journal_line_id uuid NOT NULL REFERENCES journal_lines(id),
 action text NOT NULL CHECK(action IN ('match','unmatch')),reason text NOT NULL,version bigint NOT NULL,
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),UNIQUE(line_id,version));
CREATE INDEX bank_statement_account_date_idx ON bank_statements(account_code,from_date,to_date);
CREATE INDEX bank_statement_lines_date_idx ON bank_statement_lines(transaction_date,id);
DO $$ DECLARE name text; BEGIN
 FOREACH name IN ARRAY ARRAY['treasury_accounts','treasury_transfers','bank_statements','bank_statement_lines','bank_reconciliation_events'] LOOP
 EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',name||'_immutable',name);
 EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',name||'_no_truncate',name);
 END LOOP;
END $$;
