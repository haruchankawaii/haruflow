-- New credits are separate control balances. Never change legacy cash journals.
DO $$
DECLARE role_name text; account_kind text; account_name text; chosen text; base integer;
BEGIN
 FOR role_name,account_kind,account_name,base IN VALUES
 ('customer_credit','liability','Uang muka dan kredit pelanggan',2300),
 ('supplier_advance','asset','Uang muka dan kredit supplier',1400)
 LOOP
  SELECT n::text INTO chosen FROM generate_series(base,base+99) n WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1;
  IF chosen IS NULL THEN
   WITH candidates AS (SELECT 1000000000::bigint n UNION SELECT code::bigint+1 FROM accounts WHERE code~'^[1-9][0-9]{9}$' AND code<>'9999999999')
   SELECT min(n)::text INTO chosen FROM candidates WHERE NOT EXISTS(SELECT 1 FROM accounts WHERE code=n::text);
  END IF;
  IF chosen IS NULL THEN RAISE EXCEPTION 'No unused settlement control account code'; END IF;
  INSERT INTO accounts(code,name,kind,system_role) VALUES(chosen,account_name,account_kind,role_name);
 END LOOP;
END $$;
ALTER TABLE customers ADD COLUMN payment_terms_days integer NOT NULL DEFAULT 0 CHECK(payment_terms_days BETWEEN 0 AND 3650);
ALTER TABLE invoices ADD COLUMN invoice_date date;
ALTER TABLE invoices ADD COLUMN due_date date;
ALTER TABLE invoices ADD CONSTRAINT invoice_document_dates_check CHECK(
 (invoice_date IS NULL AND due_date IS NULL) OR (invoice_date IS NOT NULL AND (due_date IS NULL OR due_date>=invoice_date)));
-- Legacy dates remain unknown rather than guessing terms at upgrade.
DO $$
DECLARE table_name text; expression text;
BEGIN
 FOREACH table_name IN ARRAY ARRAY['journal_entries','posting_reversals'] LOOP
  SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint WHERE conrelid=table_name::regclass AND conname=table_name||'_source_type_check';
  EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I',table_name,table_name||'_source_type_check');
  EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I CHECK ((%s) OR source_type IN (''party_fund'',''party_allocation'',''party_refund''))',table_name,table_name||'_source_type_check',expression);
 END LOOP;
END $$;
CREATE SEQUENCE party_fund_number;
CREATE SEQUENCE party_refund_number;
CREATE TABLE party_funds(
 id uuid PRIMARY KEY,number text NOT NULL UNIQUE,side text NOT NULL CHECK(side IN ('customer','supplier')),
 party_id uuid NOT NULL,customer_id uuid REFERENCES customers(id),supplier_id uuid REFERENCES suppliers(id),
 kind text NOT NULL CHECK(kind IN ('cash_advance','return_credit','opening_credit')),
 amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 source_type text NOT NULL CHECK(source_type IN ('party_fund','sales_return','supplier_return','opening_item')),
 source_id uuid NOT NULL,reference text,description text,order_id uuid,
 journal_id uuid NOT NULL REFERENCES journal_entries(id),created_by uuid NOT NULL REFERENCES users(id),
 version bigint NOT NULL DEFAULT 1 CHECK(version>0),created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(source_type,source_id),
 CHECK((side='customer' AND customer_id IS NOT NULL AND customer_id=party_id AND supplier_id IS NULL) OR (side='supplier' AND supplier_id IS NOT NULL AND supplier_id=party_id AND customer_id IS NULL)),
 CHECK((kind='cash_advance' AND source_type='party_fund' AND source_id=id) OR (kind='return_credit' AND source_type IN ('sales_return','supplier_return')) OR (kind='opening_credit' AND source_type='opening_item')));
CREATE INDEX party_funds_party_idx ON party_funds(side,party_id,created_at,id);
CREATE TABLE invoice_terms_changes(
 id uuid PRIMARY KEY,invoice_id uuid NOT NULL REFERENCES invoices(id),invoice_date date,due_date date,
 reason text NOT NULL,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 CHECK(invoice_date IS NULL OR due_date IS NULL OR due_date>=invoice_date));
CREATE INDEX invoice_terms_changes_invoice_idx ON invoice_terms_changes(invoice_id,created_at,id);
CREATE TRIGGER invoice_terms_changes_immutable BEFORE UPDATE OR DELETE ON invoice_terms_changes FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER invoice_terms_changes_no_truncate BEFORE TRUNCATE ON invoice_terms_changes FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TABLE party_allocations(
 id uuid PRIMARY KEY,fund_id uuid NOT NULL REFERENCES party_funds(id),
 target_type text NOT NULL CHECK(target_type IN ('invoice','bill','opening_receivable','opening_payable')),target_id uuid NOT NULL,
 target_number text NOT NULL,amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
CREATE INDEX party_allocations_target_idx ON party_allocations(target_type,target_id,created_at,id);
CREATE INDEX party_allocations_fund_idx ON party_allocations(fund_id,created_at,id);
CREATE TABLE party_target_versions(target_type text NOT NULL,target_id uuid NOT NULL,version bigint NOT NULL DEFAULT 1 CHECK(version>0),PRIMARY KEY(target_type,target_id));
CREATE TABLE party_refund_requests(
 id uuid PRIMARY KEY,fund_id uuid NOT NULL REFERENCES party_funds(id),amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 reason text NOT NULL,created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),UNIQUE(id,fund_id));
CREATE TABLE party_refund_cancellations(
 id uuid PRIMARY KEY,request_id uuid NOT NULL UNIQUE REFERENCES party_refund_requests(id),reason text NOT NULL,
 created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE party_refunds(
 id uuid PRIMARY KEY,number text NOT NULL UNIQUE,fund_id uuid NOT NULL REFERENCES party_funds(id),request_id uuid,
 amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),reference text,reason text NOT NULL,
 journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),created_by uuid NOT NULL REFERENCES users(id),created_at timestamptz NOT NULL DEFAULT now(),
 FOREIGN KEY(request_id,fund_id) REFERENCES party_refund_requests(id,fund_id));
CREATE INDEX party_refunds_fund_idx ON party_refunds(fund_id,created_at,id);
CREATE FUNCTION guard_party_fund_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.version<>OLD.version+1 OR (to_jsonb(NEW)-'version') IS DISTINCT FROM (to_jsonb(OLD)-'version') THEN RAISE EXCEPTION 'Party funding history is immutable'; END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER party_fund_version_only BEFORE UPDATE ON party_funds FOR EACH ROW EXECUTE FUNCTION guard_party_fund_update();
CREATE TRIGGER party_fund_no_delete BEFORE DELETE ON party_funds FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER party_fund_no_truncate BEFORE TRUNCATE ON party_funds FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
DO $$ DECLARE table_name text; BEGIN
 FOREACH table_name IN ARRAY ARRAY['party_allocations','party_refund_requests','party_refund_cancellations','party_refunds'] LOOP
  EXECUTE format('CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I FOR EACH ROW EXECUTE FUNCTION immutable_posting()',table_name||'_immutable',table_name);
  EXECUTE format('CREATE TRIGGER %I BEFORE TRUNCATE ON %I FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting()',table_name||'_no_truncate',table_name);
 END LOOP;
END $$;
CREATE FUNCTION party_fund_balances(target uuid) RETURNS TABLE(allocated numeric,refunded numeric,reserved numeric,available numeric) LANGUAGE sql STABLE AS $$
 WITH totals AS(SELECT f.amount,
  COALESCE((SELECT sum(a.amount) FROM party_allocations a WHERE a.fund_id=f.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_allocation' AND r.source_id=a.id)),0) allocated,
  COALESCE((SELECT sum(p.amount) FROM party_refunds p WHERE p.fund_id=f.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_refund' AND r.source_id=p.id)),0) refunded,
  COALESCE((SELECT sum(q.amount-COALESCE((SELECT sum(p.amount) FROM party_refunds p WHERE p.request_id=q.id AND NOT EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_refund' AND r.source_id=p.id)),0)) FROM party_refund_requests q WHERE q.fund_id=f.id AND NOT EXISTS(SELECT 1 FROM party_refund_cancellations c WHERE c.request_id=q.id)),0) reserved,
  EXISTS(SELECT 1 FROM posting_reversals r WHERE r.source_type='party_fund' AND r.source_id=f.id) reversed
 FROM party_funds f WHERE f.id=target)
 SELECT allocated,refunded,reserved,CASE WHEN reversed THEN 0 ELSE amount-allocated-refunded-reserved END FROM totals
$$;
CREATE FUNCTION party_reversal_document(kind text,target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',r.id,'reason',r.reason,'created_at',r.created_at,'journal_id',j.id) FROM posting_reversals r
 LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=r.id WHERE r.source_type=kind AND r.source_id=target
$$;
CREATE FUNCTION party_fund_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT to_jsonb(f)-'customer_id'-'supplier_id' || jsonb_build_object('amount',f.amount::text,
 'party_name',CASE WHEN f.side='customer' THEN c.name ELSE s.name END,
 'source_url',CASE f.source_type
  WHEN 'opening_item' THEN '?opening_item='||f.source_id::text||'#balances'
  WHEN 'sales_return' THEN (SELECT '?invoice='||r.invoice_id::text||'#sales' FROM sales_returns r WHERE r.id=f.source_id)
  WHEN 'supplier_return' THEN (SELECT '?bill='||r.bill_id::text||'#purchasing' FROM supplier_returns r WHERE r.id=f.source_id)
  WHEN 'party_fund' THEN CASE WHEN f.order_id IS NULL THEN NULL WHEN f.side='customer' THEN '?order='||f.order_id::text||'#sales' ELSE '?purchase='||f.order_id::text||'#purchasing' END
 END,
 'allocated',b.allocated::text,'refunded',b.refunded::text,'reserved',b.reserved::text,'available',b.available::text,
 'reversed',party_reversal_document('party_fund',f.id) IS NOT NULL,'reversal',party_reversal_document('party_fund',f.id),
 'allocations',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'target_type',a.target_type,'target_id',a.target_id,'number',a.target_number,'amount',a.amount::text,'journal_id',a.journal_id,'created_at',a.created_at,'reversed',party_reversal_document('party_allocation',a.id) IS NOT NULL,'reversal',party_reversal_document('party_allocation',a.id)) ORDER BY a.created_at,a.id) FROM party_allocations a WHERE a.fund_id=f.id),'[]'::jsonb),
 'refund_requests',COALESCE((SELECT jsonb_agg(to_jsonb(x) ORDER BY x.created_at,x.id) FROM (SELECT q.id,q.fund_id,q.amount::text,COALESCE(p.paid,0)::text paid,(CASE WHEN c.id IS NULL THEN q.amount-COALESCE(p.paid,0) ELSE 0 END)::text remaining,c.id IS NOT NULL cancelled,q.reason,q.created_at FROM party_refund_requests q LEFT JOIN party_refund_cancellations c ON c.request_id=q.id LEFT JOIN LATERAL(SELECT sum(p.amount) paid FROM party_refunds p WHERE p.request_id=q.id AND party_reversal_document('party_refund',p.id) IS NULL)p ON true WHERE q.fund_id=f.id)x),'[]'::jsonb),
 'refunds',COALESCE((SELECT jsonb_agg(to_jsonb(p)||jsonb_build_object('amount',p.amount::text,'reversed',party_reversal_document('party_refund',p.id) IS NOT NULL,'reversal',party_reversal_document('party_refund',p.id)) ORDER BY p.created_at,p.id) FROM party_refunds p WHERE p.fund_id=f.id),'[]'::jsonb))
 FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b LEFT JOIN customers c ON c.id=f.customer_id LEFT JOIN suppliers s ON s.id=f.supplier_id WHERE f.id=target
$$;
