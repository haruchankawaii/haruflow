-- Commercial customer returns credit the original invoice and restore historical
-- delivery cost. They do not reopen fulfillment quantities on the sales order.
ALTER TABLE invoices ADD COLUMN version bigint NOT NULL DEFAULT 1 CHECK(version>0);
ALTER TABLE invoices ADD COLUMN credit_total numeric(18,2) NOT NULL DEFAULT 0 CHECK(credit_total>=0 AND credit_total<=total);
ALTER TABLE invoices ADD CONSTRAINT invoice_net_paid_check CHECK(paid<=total-credit_total);
ALTER TABLE invoices ADD CONSTRAINT invoice_net_status_check CHECK(
    (status='void' AND paid=0 AND credit_total=0) OR
    (status='unpaid' AND paid=0 AND total-credit_total>0) OR
    (status='partial' AND paid>0 AND paid<total-credit_total) OR
    (status='paid' AND paid=total-credit_total));

-- Legacy sales deliveries had no unit snapshot. Record the catalog unit at
-- upgrade for those deliveries; new deliveries snapshot it while holding stock.
ALTER TABLE shipment_allocations ADD COLUMN product_unit text;
ALTER TABLE shipment_allocations DISABLE TRIGGER immutable_shipment_allocations;
UPDATE shipment_allocations a SET product_unit=p.unit FROM sales_order_lines l JOIN products p ON p.id=l.product_id WHERE l.id=a.line_id;
ALTER TABLE shipment_allocations ENABLE TRIGGER immutable_shipment_allocations;
ALTER TABLE shipment_allocations ALTER COLUMN product_unit SET NOT NULL;

DO $$
DECLARE table_name text; expression text;
BEGIN
  FOREACH table_name IN ARRAY ARRAY['journal_entries','posting_reversals'] LOOP
    SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint
      WHERE conrelid=table_name::regclass AND conname=table_name||'_source_type_check';
    EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I',table_name,table_name||'_source_type_check');
    EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I CHECK ((%s) OR source_type=''sales_return'')',table_name,table_name||'_source_type_check',expression);
  END LOOP;
END $$;
CREATE SEQUENCE sales_return_number;
CREATE TABLE sales_returns (
  id uuid PRIMARY KEY,
  number text NOT NULL UNIQUE,
  invoice_id uuid NOT NULL REFERENCES invoices(id),
  reason text NOT NULL CHECK(char_length(reason) BETWEEN 3 AND 1000 AND reason!~'[[:cntrl:]]'),
  credit_total numeric(18,2) NOT NULL CHECK(credit_total>0),
  refund numeric(18,2) NOT NULL CHECK(refund>=0 AND refund<=credit_total),
  cost_total numeric(18,2) NOT NULL CHECK(cost_total>=0),
  journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
  created_by uuid NOT NULL REFERENCES users(id),
  created_at timestamptz NOT NULL DEFAULT now(),
  posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id()
);
CREATE INDEX sales_returns_invoice_idx ON sales_returns(invoice_id,created_at,id);
ALTER TABLE inventory_movements ADD COLUMN sales_return_id uuid REFERENCES sales_returns(id);
DO $$
DECLARE expression text;
BEGIN
  SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint
    WHERE conrelid='inventory_movements'::regclass AND conname='inventory_movements_kind_check';
  ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_kind_check;
  EXECUTE format('ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_kind_check CHECK ((%s) OR kind IN (''sales_return'',''sales_return_reversal''))',expression);
  SELECT pg_get_expr(conbin,conrelid) INTO expression FROM pg_constraint
    WHERE conrelid='inventory_movements'::regclass AND conname='inventory_movements_direction_check';
  ALTER TABLE inventory_movements DROP CONSTRAINT inventory_movements_direction_check;
  EXECUTE format('ALTER TABLE inventory_movements ADD CONSTRAINT inventory_movements_direction_check CHECK ((%s) OR (kind=''sales_return'' AND quantity>0) OR (kind=''sales_return_reversal'' AND quantity<0))',expression);
END $$;
ALTER TABLE inventory_movements ADD CONSTRAINT sales_return_movement_reference_check
  CHECK(kind NOT IN ('sales_return','sales_return_reversal') OR sales_return_id IS NOT NULL);

CREATE TABLE sales_return_lines (
  id uuid PRIMARY KEY,
  return_id uuid NOT NULL REFERENCES sales_returns(id),
  invoice_line_id uuid NOT NULL REFERENCES invoice_allocations(id),
  product_id uuid NOT NULL REFERENCES products(id),
  product_sku text NOT NULL,
  product_name text NOT NULL,
  product_kind text NOT NULL CHECK(product_kind IN ('stock','service')),
  quantity numeric(18,3) NOT NULL CHECK(quantity>0),
  credit numeric(18,2) NOT NULL CHECK(credit>0),
  cost numeric(18,2) NOT NULL CHECK(cost>=0),
  movement_id uuid UNIQUE REFERENCES inventory_movements(id),
  before_quantity numeric(18,3),
  before_value numeric(18,2),
  before_average numeric(18,4),
  UNIQUE(return_id,invoice_line_id),
  CHECK((product_kind='service' AND cost=0 AND movement_id IS NULL AND before_quantity IS NULL AND before_value IS NULL AND before_average IS NULL)
    OR (product_kind='stock' AND movement_id IS NOT NULL AND before_quantity IS NOT NULL AND before_value IS NOT NULL AND before_average IS NOT NULL
      AND before_quantity>=0 AND before_value>=0 AND before_average>=0))
);
CREATE INDEX sales_return_lines_invoice_idx ON sales_return_lines(invoice_line_id,return_id);
CREATE TABLE sales_return_sources (
  return_line_id uuid NOT NULL REFERENCES sales_return_lines(id),
  shipment_allocation_id uuid NOT NULL REFERENCES shipment_allocations(id),
  quantity numeric(18,3) NOT NULL CHECK(quantity>0),
  cost numeric(18,2) NOT NULL CHECK(cost>=0),
  PRIMARY KEY(return_line_id,shipment_allocation_id)
);
CREATE FUNCTION guard_sales_return_append() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE return_key uuid; doc_xid xid8; doc_xmin xid; doc_invoice uuid; item_invoice uuid;
BEGIN
  IF TG_TABLE_NAME='sales_return_lines' THEN
    return_key:=NEW.return_id;
    SELECT invoice_id INTO item_invoice FROM invoice_allocations WHERE id=NEW.invoice_line_id;
  ELSE
    SELECT l.return_id,a.invoice_id INTO return_key,item_invoice FROM sales_return_lines l
      JOIN invoice_allocations a ON a.id=l.invoice_line_id WHERE l.id=NEW.return_line_id;
  END IF;
  SELECT posting_transaction,xmin,invoice_id INTO doc_xid,doc_xmin,doc_invoice FROM sales_returns WHERE id=return_key;
  IF doc_xid IS DISTINCT FROM pg_current_xact_id()
    OR doc_xmin::text::bigint<>(pg_current_xact_id()::text::numeric%4294967296)::bigint THEN
    RAISE EXCEPTION 'Cannot append lines to a posted customer return';
  END IF;
  IF item_invoice IS DISTINCT FROM doc_invoice THEN RAISE EXCEPTION 'Return line belongs to another invoice'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER sales_return_lines_original_transaction BEFORE INSERT ON sales_return_lines FOR EACH ROW EXECUTE FUNCTION guard_sales_return_append();
CREATE TRIGGER sales_return_sources_original_transaction BEFORE INSERT ON sales_return_sources FOR EACH ROW EXECUTE FUNCTION guard_sales_return_append();
CREATE TRIGGER immutable_sales_returns BEFORE UPDATE OR DELETE ON sales_returns FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_sales_returns_truncate BEFORE TRUNCATE ON sales_returns FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_sales_return_lines BEFORE UPDATE OR DELETE ON sales_return_lines FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_sales_return_lines_truncate BEFORE TRUNCATE ON sales_return_lines FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_sales_return_sources BEFORE UPDATE OR DELETE ON sales_return_sources FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_sales_return_sources_truncate BEFORE TRUNCATE ON sales_return_sources FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

CREATE FUNCTION sales_return_blocked_reason(target uuid) RETURNS text LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='sales_return' AND source_id=r.id) THEN 'Retur pelanggan sudah dibalik.'
    WHEN EXISTS(SELECT 1 FROM sales_return_lines l JOIN inventory_movements m ON m.id=l.movement_id
      WHERE l.return_id=r.id AND EXISTS(SELECT 1 FROM inventory_movements later WHERE later.product_id=l.product_id AND later.movement_seq>m.movement_seq))
      THEN 'Ada pergerakan stok setelah retur ini; koreksi melalui dokumen asalnya.'
    WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton)
      THEN 'Periode akuntansi saat ini sudah ditutup.'
    ELSE NULL END FROM sales_returns r WHERE r.id=target
$$;
CREATE FUNCTION sales_return_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object('id',r.id,'number',r.number,'invoice_id',r.invoice_id,'reason',r.reason,
    'credit_total',r.credit_total::text,'refund',r.refund::text,'cost_total',r.cost_total::text,'journal_id',r.journal_id,
    'created_at',r.created_at,'can_reverse',sales_return_blocked_reason(r.id) IS NULL,
    'reversal_blocked_reason',sales_return_blocked_reason(r.id),
    'reversal',(SELECT jsonb_build_object('id',p.id,'reason',p.reason,'created_at',p.created_at,'journal_id',j.id)
      FROM posting_reversals p LEFT JOIN journal_entries j ON j.source_type='reversal' AND j.source_id=p.id WHERE p.source_type='sales_return' AND p.source_id=r.id),
    'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',l.id,'invoice_line_id',l.invoice_line_id,
      'product_id',l.product_id,'sku',l.product_sku,'name',l.product_name,'product_kind',l.product_kind,
      'quantity',l.quantity::text,'credit',l.credit::text,'cost',l.cost::text,'movement_id',l.movement_id) ORDER BY l.product_sku)
      FROM sales_return_lines l WHERE l.return_id=r.id),'[]'::jsonb)) FROM sales_returns r WHERE r.id=target
$$;
CREATE OR REPLACE FUNCTION sales_invoice_document(target uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
 SELECT jsonb_build_object('id',i.id,'number',i.number,'order_id',i.order_id,'customer_id',i.customer_id,
 'customer_name',c.name,'status',i.status,'total',i.total::text,'paid',i.paid::text,'version',i.version,
 'credit_total',i.credit_total::text,'net_total',(i.total-i.credit_total)::text,
 'outstanding',(CASE WHEN i.status='void' THEN 0 ELSE i.total-i.credit_total-i.paid END)::text,'created_at',i.created_at,
 'can_return',i.status<>'void' AND EXISTS(SELECT 1 FROM invoice_allocations a WHERE a.invoice_id=i.id
   AND a.quantity>COALESCE((SELECT sum(l.quantity) FROM sales_return_lines l WHERE l.invoice_line_id=a.id
     AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=l.return_id)),0))
   AND (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton),
 'can_reverse',i.status<>'void' AND i.paid=0 AND i.credit_total=0
   AND (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through FROM accounting_period WHERE singleton),
 'items',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',a.id,'line_id',l.id,'product_id',l.product_id,
 'sku',l.product_sku,'name',l.product_name,'product_name',l.product_name,'product_kind',l.product_kind,
 'quantity',a.quantity::text,'unit_price',l.unit_price::text,'total',a.total::text,
 'returned_quantity',COALESCE(returned.quantity,0)::text,'returned_credit',COALESCE(returned.credit,0)::text,
 'remaining_return_quantity',(a.quantity-COALESCE(returned.quantity,0))::text) ORDER BY l.product_sku)
 FROM invoice_allocations a JOIN sales_order_lines l ON l.id=a.line_id
 LEFT JOIN LATERAL (SELECT sum(r.quantity) quantity,sum(r.credit) credit FROM sales_return_lines r
   WHERE r.invoice_line_id=a.id AND NOT EXISTS(SELECT 1 FROM posting_reversals p WHERE p.source_type='sales_return' AND p.source_id=r.return_id)) returned ON true
 WHERE a.invoice_id=i.id),'[]'::jsonb),
 'returns',COALESCE((SELECT jsonb_agg(sales_return_document(r.id) ORDER BY r.created_at,r.id) FROM sales_returns r WHERE r.invoice_id=i.id),'[]'::jsonb))
 FROM invoices i JOIN customers c ON c.id=i.customer_id WHERE i.id=target
$$;
