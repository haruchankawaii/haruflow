ALTER TABLE opening_documents ADD COLUMN parties jsonb NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(parties)='array' AND jsonb_array_length(parties)<=1000);
ALTER TABLE opening_postings ADD COLUMN parties jsonb NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(parties)='array' AND jsonb_array_length(parties)<=1000);
ALTER TABLE opening_postings ADD COLUMN posting_transaction xid8 NOT NULL DEFAULT '0';
ALTER TABLE opening_postings ALTER COLUMN posting_transaction SET DEFAULT pg_current_xact_id();
ALTER TABLE opening_documents DROP CONSTRAINT opening_documents_lines_check;
ALTER TABLE opening_documents ADD CONSTRAINT opening_documents_lines_check CHECK(jsonb_typeof(lines)='array' AND jsonb_array_length(lines)<=105);
CREATE TABLE opening_party_items (
 id uuid PRIMARY KEY,
 posting_id uuid NOT NULL REFERENCES opening_postings(id),
 kind text NOT NULL CHECK(kind IN ('receivable','payable','customer_credit','supplier_credit')),
 party_id uuid NOT NULL,
 reference text NOT NULL CHECK(char_length(reference) BETWEEN 1 AND 200 AND reference!~'[[:cntrl:]]'),
 document_date date NOT NULL,
 due_date date CHECK(due_date IS NULL OR due_date>=document_date),
 amount numeric(18,2) NOT NULL CHECK(amount>0 AND amount<=1000000000000),
 created_at timestamptz NOT NULL DEFAULT now(),
 created_by uuid NOT NULL REFERENCES users(id),
 customer_id uuid GENERATED ALWAYS AS(CASE WHEN kind IN ('receivable','customer_credit') THEN party_id END) STORED REFERENCES customers(id),
 supplier_id uuid GENERATED ALWAYS AS(CASE WHEN kind IN ('payable','supplier_credit') THEN party_id END) STORED REFERENCES suppliers(id),
 UNIQUE(posting_id,kind,party_id,reference)
);
CREATE INDEX opening_party_items_party_idx ON opening_party_items(kind,party_id,document_date,id);
CREATE UNIQUE INDEX opening_party_items_reference_idx ON opening_party_items(posting_id,kind,party_id,upper(btrim(reference)));
CREATE TRIGGER opening_party_items_immutable BEFORE UPDATE OR DELETE ON opening_party_items FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER opening_party_items_truncate BEFORE TRUNCATE ON opening_party_items FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE FUNCTION guard_opening_party_append() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NOT EXISTS(SELECT 1 FROM opening_postings p WHERE p.id=NEW.posting_id AND p.posting_transaction=pg_current_xact_id()
   AND p.xmin::text::bigint=(pg_current_xact_id()::text::numeric % 4294967296)::bigint) THEN
  RAISE EXCEPTION 'Opening party items must be inserted with their original posting' USING ERRCODE='23514';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER opening_party_item_original_posting BEFORE INSERT ON opening_party_items FOR EACH ROW EXECUTE FUNCTION guard_opening_party_append();

ALTER TABLE sales_returns ADD COLUMN settlement_mode text NOT NULL DEFAULT 'immediate' CHECK(settlement_mode IN ('immediate','credit','deferred'));
ALTER TABLE sales_returns ADD COLUMN settlement_credit numeric(18,2) NOT NULL DEFAULT 0 CHECK(settlement_credit>=0 AND settlement_credit+refund<=credit_total);
ALTER TABLE supplier_returns ADD COLUMN settlement_mode text NOT NULL DEFAULT 'immediate' CHECK(settlement_mode IN ('immediate','credit','deferred'));
ALTER TABLE supplier_returns ADD COLUMN settlement_credit numeric(18,2) NOT NULL DEFAULT 0 CHECK(settlement_credit>=0 AND settlement_credit+refund<=credit_total);
ALTER TABLE sales_returns ADD CONSTRAINT sales_return_cash_credit_mode CHECK((settlement_mode='immediate' AND settlement_credit=0) OR (settlement_mode<>'immediate' AND refund=0));
ALTER TABLE supplier_returns ADD CONSTRAINT supplier_return_cash_credit_mode CHECK((settlement_mode='immediate' AND settlement_credit=0) OR (settlement_mode<>'immediate' AND refund=0));

CREATE FUNCTION return_settlement_blocked(kind text,target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN EXISTS(SELECT 1 FROM party_funds f CROSS JOIN LATERAL party_fund_balances(f.id)b
  WHERE f.source_type=$1 AND f.source_id=$2 AND (b.allocated>0 OR b.refunded>0 OR b.reserved>0))
  THEN 'Balikkan alokasi/refund dan batalkan permintaan refund aktif sebelum membalik retur.' END
$$;
ALTER FUNCTION sales_return_blocked_reason(uuid) RENAME TO legacy_sales_return_blocked_reason;
CREATE FUNCTION sales_return_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT COALESCE(legacy_sales_return_blocked_reason(target),return_settlement_blocked('sales_return',target))
$$;
ALTER FUNCTION supplier_return_blocked_reason(uuid) RENAME TO legacy_supplier_return_blocked_reason;
CREATE FUNCTION supplier_return_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
 SELECT COALESCE(legacy_supplier_return_blocked_reason(target),return_settlement_blocked('supplier_return',target))
$$;
CREATE FUNCTION return_settlement_document(kind text,target uuid,mode text,credit numeric) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('refund_mode',mode,'settlement_mode',mode,'settlement_credit',credit::text,
 'credit_fund_id',(SELECT id FROM party_funds WHERE source_type=$1 AND source_id=$2),
 'refund_request_id',(SELECT q.id FROM party_refund_requests q JOIN party_funds f ON f.id=q.fund_id WHERE f.source_type=$1 AND f.source_id=$2 ORDER BY q.created_at,q.id LIMIT 1))
$$;
ALTER FUNCTION sales_return_document(uuid) RENAME TO legacy_sales_return_document;
CREATE FUNCTION sales_return_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT legacy_sales_return_document(target)||return_settlement_document('sales_return',r.id,r.settlement_mode,r.settlement_credit)||jsonb_build_object('can_reverse',sales_return_blocked_reason(target) IS NULL,'reversal_blocked_reason',sales_return_blocked_reason(target))
 FROM sales_returns r WHERE r.id=target
$$;
ALTER FUNCTION supplier_return_history(uuid) RENAME TO legacy_supplier_return_history;
CREATE FUNCTION supplier_return_history(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT COALESCE(jsonb_agg(item||return_settlement_document('supplier_return',r.id,r.settlement_mode,r.settlement_credit)||jsonb_build_object('can_reverse',supplier_return_blocked_reason(r.id) IS NULL,'reversal_blocked_reason',supplier_return_blocked_reason(r.id)) ORDER BY r.created_at,r.id),'[]'::jsonb)
 FROM jsonb_array_elements(legacy_supplier_return_history(target))item JOIN supplier_returns r ON r.id=(item->>'id')::uuid
$$;

ALTER TABLE document_attachments DROP CONSTRAINT document_attachments_document_type_check;
ALTER TABLE document_attachments ADD CONSTRAINT document_attachments_document_type_check CHECK(document_type IN ('sales_order','invoice','payment','purchase_order','purchase_receipt','purchase_bill','supplier_payment','journal','sales_return','supplier_return','party_fund','party_refund','opening_item'));
ALTER TABLE document_attachments ADD COLUMN party_fund_id uuid GENERATED ALWAYS AS(CASE WHEN document_type='party_fund' THEN document_id END) STORED REFERENCES party_funds(id);
ALTER TABLE document_attachments ADD COLUMN party_refund_id uuid GENERATED ALWAYS AS(CASE WHEN document_type='party_refund' THEN document_id END) STORED REFERENCES party_refunds(id);
ALTER TABLE document_attachments ADD COLUMN opening_item_id uuid GENERATED ALWAYS AS(CASE WHEN document_type='opening_item' THEN document_id END) STORED REFERENCES opening_party_items(id);
