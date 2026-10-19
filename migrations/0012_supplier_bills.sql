-- Supplier bills reclassify received goods accruals to payable balances. They do
-- not receive stock again or move cash. Preserve user-created account codes.
DO $$
DECLARE payable_code text;
BEGIN
    SELECT n::text INTO payable_code FROM generate_series(2200,2299) n
        WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE code=n::text) ORDER BY n LIMIT 1;
    IF payable_code IS NULL THEN
        WITH candidates AS (
            SELECT 1000000000::bigint n
            UNION
            SELECT code::bigint+1 FROM accounts
                WHERE code ~ '^[1-9][0-9]{9}$' AND code<>'9999999999'
        )
        SELECT min(n)::text INTO payable_code FROM candidates
            WHERE NOT EXISTS (SELECT 1 FROM accounts WHERE code=n::text);
    END IF;
    IF payable_code IS NULL THEN RAISE EXCEPTION 'No unused supplier payable account code'; END IF;
    INSERT INTO accounts(code,name,kind,system_role)
        VALUES(payable_code,'Utang supplier','liability','purchase_payable');
END $$;

ALTER TABLE journal_entries DROP CONSTRAINT journal_entries_source_type_check;
ALTER TABLE journal_entries ADD CONSTRAINT journal_entries_source_type_check
    CHECK (source_type IN ('receipt','shipment','invoice','payment','manual','adjustment','reversal','opening','purchase_receipt','purchase_bill'));
ALTER TABLE posting_reversals DROP CONSTRAINT posting_reversals_source_type_check;
ALTER TABLE posting_reversals ADD CONSTRAINT posting_reversals_source_type_check
    CHECK (source_type IN ('payment','invoice','shipment','opening','manual','receipt','purchase_receipt','purchase_bill'));

CREATE SEQUENCE purchase_bill_number;
CREATE TABLE purchase_bills (
    id uuid PRIMARY KEY,
    number text NOT NULL UNIQUE,
    receipt_id uuid NOT NULL REFERENCES purchase_receipts(id),
    order_id uuid NOT NULL REFERENCES purchase_orders(id),
    supplier_id uuid NOT NULL REFERENCES suppliers(id),
    supplier_code text NOT NULL,
    supplier_name text NOT NULL,
    supplier_invoice_number text NOT NULL CHECK (char_length(supplier_invoice_number) BETWEEN 1 AND 100
        AND supplier_invoice_number=btrim(supplier_invoice_number) AND supplier_invoice_number !~ '[[:cntrl:]]'),
    invoice_date date NOT NULL CHECK (invoice_date BETWEEN DATE '0001-01-01' AND DATE '9999-12-31'),
    due_date date NOT NULL CHECK (due_date BETWEEN DATE '0001-01-01' AND DATE '9999-12-31' AND due_date>=invoice_date),
    notes text CHECK (notes IS NULL OR char_length(notes) BETWEEN 1 AND 2000),
    total numeric(18,2) NOT NULL CHECK (total>0 AND total<=1000000000000),
    status text NOT NULL DEFAULT 'unpaid' CHECK (status IN ('unpaid','void')),
    version integer NOT NULL DEFAULT 1 CHECK (version>0),
    journal_id uuid NOT NULL UNIQUE REFERENCES journal_entries(id),
    created_by uuid NOT NULL REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY(receipt_id,order_id) REFERENCES purchase_receipts(id,order_id)
);
CREATE UNIQUE INDEX purchase_bills_active_receipt_idx ON purchase_bills(receipt_id) WHERE status='unpaid';
CREATE UNIQUE INDEX purchase_bills_active_invoice_idx
    ON purchase_bills(supplier_id,lower(btrim(supplier_invoice_number))) WHERE status='unpaid';
CREATE INDEX purchase_bills_created_idx ON purchase_bills(created_at DESC,id);
CREATE INDEX purchase_bills_due_idx ON purchase_bills(due_date,id) WHERE status='unpaid';
CREATE FUNCTION guard_purchase_bill_update() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status<>'unpaid' OR NEW.status<>'void' OR NEW.version<>OLD.version+1
        OR (to_jsonb(NEW)-'status'-'version') IS DISTINCT FROM (to_jsonb(OLD)-'status'-'version') THEN
        RAISE EXCEPTION 'Posted supplier bills are immutable; reverse the bill instead';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER guard_purchase_bill_update BEFORE UPDATE ON purchase_bills
    FOR EACH ROW EXECUTE FUNCTION guard_purchase_bill_update();
CREATE TRIGGER immutable_purchase_bills_delete BEFORE DELETE ON purchase_bills
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_purchase_bills_truncate BEFORE TRUNCATE ON purchase_bills
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();

CREATE FUNCTION purchase_bill_blocked_reason(bill uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN b.status='void' THEN 'Tagihan supplier sudah dibalik.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_bills b WHERE b.id=bill
$$;

CREATE FUNCTION purchase_receipt_bill_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status<>'received' OR o.active_receipt_id IS DISTINCT FROM r.id
            THEN 'Penerimaan ini bukan penerimaan aktif PO.'
        WHEN EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status='unpaid')
            THEN 'Penerimaan ini sudah memiliki tagihan supplier aktif.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;

-- A bill must first be reversed before its receipt can be corrected. Subsequent
-- stock activity blocks receipt reversal but deliberately does not block billing.
CREATE OR REPLACE FUNCTION purchase_receipt_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status<>'received' OR o.active_receipt_id IS DISTINCT FROM r.id
            THEN 'Penerimaan ini bukan penerimaan aktif PO.'
        WHEN EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status='unpaid')
            THEN 'Balikkan tagihan supplier aktif sebelum membalik penerimaan ini.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN inventory_movements m ON m.id=l.movement_id
            WHERE l.receipt_id=r.id AND EXISTS(SELECT 1 FROM inventory_movements later
                WHERE later.product_id=l.product_id AND later.movement_seq>m.movement_seq))
            THEN 'Ada mutasi stok setelah penerimaan ini; koreksi tidak dapat memutar ulang HPP historis.'
        WHEN EXISTS(SELECT 1 FROM purchase_receipt_lines l JOIN products p ON p.id=l.product_id
            WHERE l.receipt_id=r.id AND (p.quantity<>l.before_quantity+l.quantity
                OR p.inventory_value<>l.before_inventory_value+l.value
                OR p.average_cost<>round((l.before_inventory_value+l.value)/(l.before_quantity+l.quantity),4)))
            THEN 'Saldo stok tidak lagi cocok dengan snapshot penerimaan.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton)
            THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;

CREATE OR REPLACE FUNCTION purchase_receipt_history(purchase_order uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'id',r.id,'number',r.number,'order_id',r.order_id,'reference',r.reference,'total',r.total::text,
        'journal_id',r.journal_id,'created_at',r.created_at,
        'can_reverse',purchase_receipt_blocked_reason(r.id) IS NULL,
        'reversal_blocked_reason',purchase_receipt_blocked_reason(r.id),
        'bill_id',b.id,'bill_number',b.number,
        'can_bill',purchase_receipt_bill_blocked_reason(r.id) IS NULL,
        'bill_blocked_reason',purchase_receipt_bill_blocked_reason(r.id),
        'reversal',CASE WHEN rv.id IS NULL THEN NULL ELSE jsonb_build_object(
            'id',rv.id,'reason',rv.reason,'created_at',rv.created_at,'journal_id',rj.id) END,
        'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
            'id',l.id,'movement_id',l.movement_id,'product_id',l.product_id,'product_sku',l.product_sku,
            'product_name',l.product_name,'unit',l.unit,'quantity',l.quantity::text,'unit_cost',l.unit_cost::text,'value',l.value::text)
            ORDER BY l.position) FROM purchase_receipt_lines l WHERE l.receipt_id=r.id),'[]'::jsonb)
        ) ORDER BY r.created_at DESC,r.number DESC),'[]'::jsonb)
    FROM purchase_receipts r
    LEFT JOIN posting_reversals rv ON rv.source_type='purchase_receipt' AND rv.source_id=r.id
    LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=rv.id
    LEFT JOIN purchase_bills b ON b.receipt_id=r.id AND b.status='unpaid'
    WHERE r.order_id=purchase_order
$$;
