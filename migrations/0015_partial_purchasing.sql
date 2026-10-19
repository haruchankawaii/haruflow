-- Allocate receipts and supplier bills by immutable line quantities. Existing
-- full receipts/bills retain their original values and journal postings.
ALTER TABLE purchase_orders DROP CONSTRAINT purchase_orders_status_check;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_status_check
    CHECK(status IN ('draft','confirmed','partial_received','received','cancelled'));
ALTER TABLE purchase_orders DROP CONSTRAINT purchase_orders_state_check;
ALTER TABLE purchase_orders ADD CONSTRAINT purchase_orders_state_check CHECK (
    (status='draft' AND confirmed_at IS NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NULL AND active_receipt_id IS NULL) OR
    (status='confirmed' AND confirmed_at IS NOT NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NULL AND active_receipt_id IS NULL) OR
    (status IN ('partial_received','received') AND confirmed_at IS NOT NULL AND cancelled_at IS NULL AND cancellation_reason IS NULL AND received_at IS NOT NULL AND active_receipt_id IS NOT NULL) OR
    (status='cancelled' AND cancelled_at IS NOT NULL AND cancellation_reason IS NOT NULL AND received_at IS NULL AND active_receipt_id IS NULL));

ALTER TABLE purchase_bills ADD COLUMN posting_transaction xid8 NOT NULL DEFAULT pg_current_xact_id();
DROP INDEX purchase_bills_active_receipt_idx;
CREATE TABLE purchase_bill_lines (
    id uuid PRIMARY KEY,
    bill_id uuid NOT NULL REFERENCES purchase_bills(id),
    receipt_line_id uuid NOT NULL REFERENCES purchase_receipt_lines(id),
    position integer NOT NULL CHECK(position BETWEEN 1 AND 100),
    product_id uuid NOT NULL REFERENCES products(id),
    product_sku text NOT NULL,
    product_name text NOT NULL,
    unit text NOT NULL,
    quantity numeric(18,3) NOT NULL CHECK(quantity>0 AND quantity<=1000000000),
    unit_cost numeric(18,4) NOT NULL CHECK(unit_cost>0 AND unit_cost<=1000000000),
    value numeric(18,2) NOT NULL CHECK(value>0 AND value<=1000000000000),
    UNIQUE(bill_id,receipt_line_id),
    UNIQUE(bill_id,position)
);
INSERT INTO purchase_bill_lines(id,bill_id,receipt_line_id,position,product_id,product_sku,product_name,unit,quantity,unit_cost,value)
    SELECT gen_random_uuid(),b.id,l.id,l.position,l.product_id,l.product_sku,l.product_name,l.unit,l.quantity,l.unit_cost,l.value
    FROM purchase_bills b JOIN purchase_receipt_lines l ON l.receipt_id=b.receipt_id;
CREATE INDEX purchase_bill_lines_receipt_idx ON purchase_bill_lines(receipt_line_id);
CREATE TRIGGER immutable_purchase_bill_lines BEFORE UPDATE OR DELETE ON purchase_bill_lines
    FOR EACH ROW EXECUTE FUNCTION immutable_posting();
CREATE TRIGGER immutable_purchase_bill_lines_truncate BEFORE TRUNCATE ON purchase_bill_lines
    FOR EACH STATEMENT EXECUTE FUNCTION immutable_posting();
CREATE FUNCTION guard_purchase_bill_append() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE current_transaction xid8 := pg_current_xact_id();
BEGIN
    IF NOT EXISTS(SELECT 1 FROM purchase_bills b JOIN purchase_receipt_lines l ON l.id=NEW.receipt_line_id
        WHERE b.id=NEW.bill_id AND b.receipt_id=l.receipt_id
          AND b.posting_transaction=current_transaction
          AND b.xmin::text::numeric=mod(current_transaction::text::numeric,4294967296)) THEN
        RAISE EXCEPTION 'Cannot append lines to a posted supplier bill';
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER guard_purchase_bill_lines BEFORE INSERT ON purchase_bill_lines
    FOR EACH ROW EXECUTE FUNCTION guard_purchase_bill_append();

CREATE FUNCTION purchase_order_received_quantity(purchase_order uuid,product uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT COALESCE(sum(l.quantity),0) FROM purchase_receipt_lines l JOIN purchase_receipts r ON r.id=l.receipt_id
    WHERE r.order_id=purchase_order AND l.product_id=product AND NOT EXISTS(
        SELECT 1 FROM posting_reversals v WHERE v.source_type='purchase_receipt' AND v.source_id=r.id)
$$;
CREATE FUNCTION purchase_order_received_value(purchase_order uuid,product uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT COALESCE(sum(l.value),0) FROM purchase_receipt_lines l JOIN purchase_receipts r ON r.id=l.receipt_id
    WHERE r.order_id=purchase_order AND l.product_id=product AND NOT EXISTS(
        SELECT 1 FROM posting_reversals v WHERE v.source_type='purchase_receipt' AND v.source_id=r.id)
$$;
CREATE FUNCTION purchase_receipt_billed_quantity(receipt_line uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT COALESCE(sum(l.quantity),0) FROM purchase_bill_lines l JOIN purchase_bills b ON b.id=l.bill_id
    WHERE l.receipt_line_id=receipt_line AND b.status<>'void'
$$;
CREATE FUNCTION purchase_receipt_billed_value(receipt_line uuid) RETURNS numeric LANGUAGE sql STABLE AS $$
    SELECT COALESCE(sum(l.value),0) FROM purchase_bill_lines l JOIN purchase_bills b ON b.id=l.bill_id
    WHERE l.receipt_line_id=receipt_line AND b.status<>'void'
$$;
CREATE OR REPLACE FUNCTION purchase_receipt_bill_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status NOT IN ('partial_received','received') THEN 'PO tidak memiliki penerimaan aktif.'
        WHEN NOT EXISTS(SELECT 1 FROM purchase_receipt_lines l WHERE l.receipt_id=r.id
            AND l.quantity>purchase_receipt_billed_quantity(l.id)) THEN 'Seluruh barang penerimaan ini sudah ditagih.'
        WHEN NOT (SELECT closed_through IS NULL OR (now() AT TIME ZONE (SELECT timezone FROM company))::date>closed_through
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;
CREATE OR REPLACE FUNCTION purchase_receipt_blocked_reason(receipt uuid) RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN EXISTS(SELECT 1 FROM posting_reversals WHERE source_type='purchase_receipt' AND source_id=r.id)
            THEN 'Penerimaan PO sudah dibalik.'
        WHEN o.status NOT IN ('partial_received','received') THEN 'PO tidak memiliki penerimaan aktif.'
        WHEN EXISTS(SELECT 1 FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void')
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
            FROM accounting_period WHERE singleton) THEN 'Periode akuntansi saat ini sudah ditutup.'
        ELSE NULL END
    FROM purchase_receipts r JOIN purchase_orders o ON o.id=r.order_id WHERE r.id=receipt
$$;
CREATE OR REPLACE FUNCTION purchase_receipt_history(purchase_order uuid) RETURNS jsonb LANGUAGE sql STABLE AS $$
    SELECT COALESCE(jsonb_agg(jsonb_build_object(
        'id',r.id,'number',r.number,'order_id',r.order_id,'reference',r.reference,'total',r.total::text,
        'journal_id',r.journal_id,'created_at',r.created_at,
        'can_reverse',purchase_receipt_blocked_reason(r.id) IS NULL,
        'reversal_blocked_reason',purchase_receipt_blocked_reason(r.id),
        'bill_id',(SELECT b.id FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void' ORDER BY b.created_at,b.id LIMIT 1),
        'bill_number',(SELECT b.number FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void' ORDER BY b.created_at,b.id LIMIT 1),
        'bills',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',b.id,'number',b.number,'status',b.status,'total',b.total::text)
            ORDER BY b.created_at,b.id) FROM purchase_bills b WHERE b.receipt_id=r.id AND b.status<>'void'),'[]'::jsonb),
        'can_bill',purchase_receipt_bill_blocked_reason(r.id) IS NULL,
        'bill_blocked_reason',purchase_receipt_bill_blocked_reason(r.id),
        'reversal',CASE WHEN rv.id IS NULL THEN NULL ELSE jsonb_build_object(
            'id',rv.id,'reason',rv.reason,'created_at',rv.created_at,'journal_id',rj.id) END,
        'items',COALESCE((SELECT jsonb_agg(jsonb_build_object(
            'id',l.id,'movement_id',l.movement_id,'product_id',l.product_id,'product_sku',l.product_sku,
            'product_name',l.product_name,'unit',l.unit,'quantity',l.quantity::text,'unit_cost',l.unit_cost::text,'value',l.value::text,
            'billed_quantity',purchase_receipt_billed_quantity(l.id)::text,
            'remaining_bill_quantity',(l.quantity-purchase_receipt_billed_quantity(l.id))::text,
            'billed_value',purchase_receipt_billed_value(l.id)::text)
            ORDER BY l.position) FROM purchase_receipt_lines l WHERE l.receipt_id=r.id),'[]'::jsonb)
        ) ORDER BY r.created_at DESC,r.number DESC),'[]'::jsonb)
    FROM purchase_receipts r
    LEFT JOIN posting_reversals rv ON rv.source_type='purchase_receipt' AND rv.source_id=r.id
    LEFT JOIN journal_entries rj ON rj.source_type='reversal' AND rj.source_id=rv.id
    WHERE r.order_id=purchase_order
$$;
